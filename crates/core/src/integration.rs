use core::fmt;
use core::future::Future;

use crate::catalogue::ProjectId;
use crate::explore::ReadError;
use crate::identity::AccountId;
use crate::membership::{Memberships, OrgRole, Whoami, WhoamiError};
use crate::oidc::FlowError;
use crate::secret::SessionToken;
use crate::session::SessionStore;
use crate::tenant::TenantId;
use crate::vault::{ProviderToken, SealError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Provider {
    Linear,
    Github,
    Jira,
}

impl Provider {
    /// The spellings the schema holds, which are also the ones the api and the screen carry.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Github => "github",
            Self::Jira => "jira",
        }
    }

    pub fn read(spelling: &str) -> Option<Self> {
        match spelling {
            "linear" => Some(Self::Linear),
            "github" => Some(Self::Github),
            "jira" => Some(Self::Jira),
            _ => None,
        }
    }

    /// Every one of them, so a screen can ask about the ones that are not connected too.
    pub fn every() -> [Self; 3] {
        [Self::Linear, Self::Github, Self::Jira]
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/* What a connection card shows, and not one field of it is a secret. The label is the workspace
or site the connection is to: "Connected" with nothing beside it says nothing worth reading, and
an organisation with two Jira sites needs to know which one this is. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub provider: Provider,
    pub account_label: Box<str>,
    pub connected_at_unix_nanos: u64,
    pub connected_by: Option<AccountId>,
}

/* The credential is handed about whole rather than as an access token, because the three
providers hand back different things — Jira a token, a refresh token and an expiry, the others
less — and the shape inside stays theirs to decide. What this layer guarantees is that it is
sealed on the way in and opened on the way out, and that the store never holds it in the clear. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connecting<'a> {
    pub provider: Provider,
    pub account_label: &'a str,
    pub credential: &'a ProviderToken,
    pub by: AccountId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConnectionError {
    #[error("no live session carries this token")]
    NoSession,
    #[error("a store could not be reached")]
    StoreUnreachable,
    #[error("this deployment holds no key to seal a connection with")]
    NoKey,
    /* The key changed, or the row came from another deployment. Worth its own answer because the
    screen can say the one useful thing about it: connect it again. */
    #[error("this connection does not open under this deployment's key")]
    Unopenable,
    #[error("no connect attempt is waiting under this state")]
    NoSuchAttempt,
    #[error("the provider refused the handshake")]
    ProviderRefused,
    #[error("the provider could not be reached")]
    ProviderUnreachable,
    #[error("this deployment has registered no application with that provider")]
    ProviderUnconfigured,
    #[error("this session holds no membership of that organisation")]
    NotYours,
    #[error("connecting a provider is an administrator's to do")]
    NotYoursToChange,
    #[error("nothing is connected to that provider")]
    NotConnected,
    #[error("that provider offers no such place to file an issue")]
    NoSuchDestination,
}

impl From<FlowError> for ConnectionError {
    fn from(error: FlowError) -> Self {
        match error {
            FlowError::Unknown => Self::NoSuchAttempt,
            FlowError::StoreUnreachable => Self::StoreUnreachable,
        }
    }
}

impl From<WhoamiError> for ConnectionError {
    fn from(error: WhoamiError) -> Self {
        match error {
            WhoamiError::NoSession => Self::NoSession,
            WhoamiError::StoreUnreachable => Self::StoreUnreachable,
        }
    }
}

impl From<ReadError> for ConnectionError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::StoreUnreachable => Self::StoreUnreachable,
        }
    }
}

impl From<SealError> for ConnectionError {
    fn from(error: SealError) -> Self {
        match error {
            SealError::NoKey => Self::NoKey,
            SealError::Unsealable | SealError::Unopenable => Self::Unopenable,
        }
    }
}

/* A connect attempt, kept server side for one use. The state is the only thing the provider
hands back, so it is the only thing the callback is believed about: which organisation this is
for, and who started it, are read from the row.

That is the whole point of the row. A callback that could name its own organisation would let
anybody able to start a flow attach a credential to somebody else's. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub state: String,
    pub org: TenantId,
    pub by: AccountId,
    pub provider: Provider,
}

pub trait Attempts {
    fn begin(&self, attempt: &Attempt) -> impl Future<Output = Result<(), FlowError>> + Send;

    /// Spent on the way out: a state that comes back twice is a state that works once.
    fn take(&self, state: &str) -> impl Future<Output = Result<Attempt, FlowError>> + Send;
}

impl<T: Attempts + Sync> Attempts for &T {
    fn begin(&self, attempt: &Attempt) -> impl Future<Output = Result<(), FlowError>> + Send {
        (*self).begin(attempt)
    }

    fn take(&self, state: &str) -> impl Future<Output = Result<Attempt, FlowError>> + Send {
        (*self).take(state)
    }
}

impl<T: Attempts + Send + Sync> Attempts for std::sync::Arc<T> {
    fn begin(&self, attempt: &Attempt) -> impl Future<Output = Result<(), FlowError>> + Send {
        self.as_ref().begin(attempt)
    }

    fn take(&self, state: &str) -> impl Future<Output = Result<Attempt, FlowError>> + Send {
        self.as_ref().take(state)
    }
}

/* What a finished handshake yields: the credential to seal, and the name of the workspace or site
it is to. The label is asked of the provider rather than typed by anybody, because a connection
somebody labelled themselves would drift from what it actually reaches. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorized {
    pub credential: ProviderToken,
    pub account_label: Box<str>,
}

/* The provider conversation. Where to send the browser, and what to do with what comes back —
the two halves of an authorization code flow and nothing else. Each provider implements it its
own way, and none of them tells this layer how. */
pub trait Handshake {
    fn authorize_url(&self, provider: Provider, state: &str) -> Result<String, ConnectionError>;

    fn finish(
        &self,
        provider: Provider,
        code: &str,
    ) -> impl Future<Output = Result<Authorized, ConnectionError>> + Send;
}

impl<T: Handshake + Sync> Handshake for &T {
    fn authorize_url(&self, provider: Provider, state: &str) -> Result<String, ConnectionError> {
        (*self).authorize_url(provider, state)
    }

    fn finish(
        &self,
        provider: Provider,
        code: &str,
    ) -> impl Future<Output = Result<Authorized, ConnectionError>> + Send {
        (*self).finish(provider, code)
    }
}

impl<T: Handshake + Send + Sync> Handshake for std::sync::Arc<T> {
    fn authorize_url(&self, provider: Provider, state: &str) -> Result<String, ConnectionError> {
        self.as_ref().authorize_url(provider, state)
    }

    fn finish(
        &self,
        provider: Provider,
        code: &str,
    ) -> impl Future<Output = Result<Authorized, ConnectionError>> + Send {
        self.as_ref().finish(provider, code)
    }
}

pub trait Connections {
    /// What is connected, with nothing secret in it. The ones that are not connected are absent.
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Connection>, ConnectionError>> + Send;

    /// Connecting again replaces what was there, which is what reconnecting means.
    fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send;

    /* Opened, because the only caller is about to call the provider with it. Nothing else in the
    system reads this, and nothing logs it. */
    fn credential(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<ProviderToken>, ConnectionError>> + Send;

    /// Whether a connection was there to drop.
    fn disconnect(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<bool, ConnectionError>> + Send;
}

impl<T: Connections + Sync> Connections for &T {
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Connection>, ConnectionError>> + Send {
        (*self).of(org)
    }

    fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send {
        (*self).connect(org, connecting)
    }

    fn credential(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<ProviderToken>, ConnectionError>> + Send {
        (*self).credential(org, provider)
    }

    fn disconnect(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<bool, ConnectionError>> + Send {
        (*self).disconnect(org, provider)
    }
}

impl<T: Connections + Send + Sync> Connections for std::sync::Arc<T> {
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Connection>, ConnectionError>> + Send {
        self.as_ref().of(org)
    }

    fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send {
        self.as_ref().connect(org, connecting)
    }

    fn credential(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<ProviderToken>, ConnectionError>> + Send {
        self.as_ref().credential(org, provider)
    }

    fn disconnect(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<bool, ConnectionError>> + Send {
        self.as_ref().disconnect(org, provider)
    }
}

impl Attempt {
    /* The state is drawn here rather than by a caller, so there is one place that decides how
    unguessable it is. Thirty-two bytes, like every other secret this system mints. */
    pub fn fresh(org: TenantId, by: AccountId, provider: Provider) -> Self {
        let bytes: [u8; 32] = rand::random();
        Self {
            state: hex::encode(bytes),
            org,
            by,
            provider,
        }
    }
}

/* Connecting a provider to an organisation. Three checks live here and each one is load bearing.

Starting is an administrator's, in that organisation. Finishing believes the state and nothing
else in the query string — the organisation and the account come out of the row. And the role is
read again on the way back, because a flow started on Monday and finished on Tuesday was started
by somebody who may since have been removed; the attempt row says who they were, not what they
still are. */
#[derive(Debug)]
pub struct Linking<S, G, C, A, H> {
    whoami: Whoami<S, G>,
    memberships: G,
    connections: C,
    attempts: A,
    handshake: H,
}

impl<S, G, C, A, H> Linking<S, G, C, A, H>
where
    S: SessionStore,
    G: Memberships + Clone,
    C: Connections,
    A: Attempts,
    H: Handshake,
{
    pub fn new(sessions: S, memberships: G, connections: C, attempts: A, handshake: H) -> Self {
        Self {
            whoami: Whoami::new(sessions, memberships.clone()),
            memberships,
            connections,
            attempts,
            handshake,
        }
    }

    pub async fn connections(
        &self,
        token: &SessionToken,
        org: TenantId,
    ) -> Result<Vec<Connection>, ConnectionError> {
        self.standing(token, org, OrgRole::Member).await?;
        self.connections.of(org).await
    }

    /// Answers where to send the browser, having written down what this attempt is for.
    pub async fn start(
        &self,
        token: &SessionToken,
        org: TenantId,
        provider: Provider,
    ) -> Result<String, ConnectionError> {
        let who = self.standing(token, org, OrgRole::Admin).await?;
        let attempt = Attempt::fresh(org, who, provider);
        /* The url is built before the row is written, so a provider this deployment registered no
        application with leaves no attempt behind to expire. */
        let url = self.handshake.authorize_url(provider, &attempt.state)?;
        self.attempts.begin(&attempt).await?;
        Ok(url)
    }

    /* No session reaches this: the browser arrives from the provider, carrying a state and a
    code. The state is spent on the way in, so the same callback replayed does nothing. */
    pub async fn finish(
        &self,
        provider: Provider,
        state: &str,
        code: &str,
    ) -> Result<Connection, ConnectionError> {
        let attempt = self.attempts.take(state).await?;
        /* A state minted for one provider must not be redeemable at another's callback. */
        if attempt.provider != provider {
            return Err(ConnectionError::NoSuchAttempt);
        }
        if !self.still_an_administrator(attempt.by, attempt.org).await? {
            return Err(ConnectionError::NotYoursToChange);
        }

        let authorized = self.handshake.finish(provider, code).await?;
        self.connections
            .connect(
                attempt.org,
                Connecting {
                    provider,
                    account_label: &authorized.account_label,
                    credential: &authorized.credential,
                    by: attempt.by,
                },
            )
            .await?;
        Ok(Connection {
            provider,
            account_label: authorized.account_label,
            connected_at_unix_nanos: 0,
            connected_by: Some(attempt.by),
        })
    }

    pub async fn disconnect(
        &self,
        token: &SessionToken,
        org: TenantId,
        provider: Provider,
    ) -> Result<(), ConnectionError> {
        self.standing(token, org, OrgRole::Admin).await?;
        if self.connections.disconnect(org, provider).await? {
            Ok(())
        } else {
            Err(ConnectionError::NotConnected)
        }
    }

    async fn standing(
        &self,
        token: &SessionToken,
        org: TenantId,
        floor: OrgRole,
    ) -> Result<AccountId, ConnectionError> {
        let who = self.whoami.of(token).await?;
        match who.role_in(org) {
            None => Err(ConnectionError::NotYours),
            Some(role) if !role.at_least(floor) => Err(ConnectionError::NotYoursToChange),
            Some(_) => Ok(who.account),
        }
    }

    async fn still_an_administrator(
        &self,
        account: AccountId,
        org: TenantId,
    ) -> Result<bool, ConnectionError> {
        Ok(self
            .memberships
            .of(account)
            .await?
            .iter()
            .find(|membership| membership.org == org)
            .is_some_and(|membership| membership.role.at_least(OrgRole::Admin)))
    }
}

/* Somewhere an issue can be filed: a Linear team, a GitHub repository, a Jira project. The id is
the provider's and is the only half ever sent back to it; the name is carried so a screen can say
"Engineering" without calling the provider to render a table. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub id: Box<str>,
    pub name: Box<str>,
}

/* Where issues go for one provider: what the organisation chose, and the projects that differ.
Reading the two separately is deliberate — a screen has to show which projects are following the
default, and a resolved value alone cannot say that. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routing {
    pub provider: Provider,
    pub default: Option<Destination>,
    pub projects: Vec<(ProjectId, Destination)>,
}

/* Asked of the provider, with the credential the organisation connected. Separate from the
handshake because it is a different moment: the handshake happens once, this happens every time
somebody opens the chooser, and teams get created and archived in between. */
pub trait Destinations {
    fn offered(
        &self,
        provider: Provider,
        credential: &ProviderToken,
    ) -> impl Future<Output = Result<Vec<Destination>, ConnectionError>> + Send;
}

impl<T: Destinations + Sync> Destinations for &T {
    fn offered(
        &self,
        provider: Provider,
        credential: &ProviderToken,
    ) -> impl Future<Output = Result<Vec<Destination>, ConnectionError>> + Send {
        (*self).offered(provider, credential)
    }
}

impl<T: Destinations + Send + Sync> Destinations for std::sync::Arc<T> {
    fn offered(
        &self,
        provider: Provider,
        credential: &ProviderToken,
    ) -> impl Future<Output = Result<Vec<Destination>, ConnectionError>> + Send {
        self.as_ref().offered(provider, credential)
    }
}

pub trait Filings {
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Routing>, ConnectionError>> + Send;

    /* `project` of nothing is the organisation's default, and `destination` of nothing clears
    whichever was named — a project with nothing of its own follows the default again, and an
    organisation with no default has nowhere to file until somebody picks one. One method, because
    the two are the same act at two altitudes. */
    fn aim(
        &self,
        org: TenantId,
        provider: Provider,
        project: Option<ProjectId>,
        destination: Option<&Destination>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send;
}

impl<T: Filings + Sync> Filings for &T {
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Routing>, ConnectionError>> + Send {
        (*self).of(org)
    }

    fn aim(
        &self,
        org: TenantId,
        provider: Provider,
        project: Option<ProjectId>,
        destination: Option<&Destination>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send {
        (*self).aim(org, provider, project, destination)
    }
}

impl<T: Filings + Send + Sync> Filings for std::sync::Arc<T> {
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Routing>, ConnectionError>> + Send {
        self.as_ref().of(org)
    }

    fn aim(
        &self,
        org: TenantId,
        provider: Provider,
        project: Option<ProjectId>,
        destination: Option<&Destination>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send {
        self.as_ref().aim(org, provider, project, destination)
    }
}

/* Choosing where issues go. Reading is any member's; choosing is an administrator's, like every
other arrangement of an organisation.

Asking what is on offer needs the credential, so it is an administrator's too — and it is the one
read here that leaves the building. */
#[derive(Debug)]
pub struct Filing<S, G, C, F, D> {
    whoami: Whoami<S, G>,
    connections: C,
    filings: F,
    destinations: D,
}

impl<S, G, C, F, D> Filing<S, G, C, F, D>
where
    S: SessionStore,
    G: Memberships,
    C: Connections,
    F: Filings,
    D: Destinations,
{
    pub fn new(sessions: S, memberships: G, connections: C, filings: F, destinations: D) -> Self {
        Self {
            whoami: Whoami::new(sessions, memberships),
            connections,
            filings,
            destinations,
        }
    }

    pub async fn routings(
        &self,
        token: &SessionToken,
        org: TenantId,
    ) -> Result<Vec<Routing>, ConnectionError> {
        self.standing(token, org, OrgRole::Member).await?;
        self.filings.of(org).await
    }

    /* The chooser's list, asked of the provider every time. A team created a minute ago should be
    on offer, and one archived a minute ago should not. */
    pub async fn offered(
        &self,
        token: &SessionToken,
        org: TenantId,
        provider: Provider,
    ) -> Result<Vec<Destination>, ConnectionError> {
        self.standing(token, org, OrgRole::Admin).await?;
        self.offering(org, provider).await
    }

    /* The id is the provider's and is taken as given; the name is read off the list rather than
    out of the request, so a screen cannot label a destination something it is not. Which also
    means a destination that is no longer on offer cannot be chosen.

    Clearing takes neither branch of that: it asks the provider nothing, which matters on the day
    the provider is down — somebody who wants to stop filing issues somewhere should not be held
    up by the place they are trying to stop filing them. */
    pub async fn aim(
        &self,
        token: &SessionToken,
        org: TenantId,
        provider: Provider,
        project: Option<ProjectId>,
        destination: Option<&str>,
    ) -> Result<Vec<Routing>, ConnectionError> {
        self.standing(token, org, OrgRole::Admin).await?;

        let Some(wanted) = destination else {
            self.filings.aim(org, provider, project, None).await?;
            return self.filings.of(org).await;
        };

        let named = self
            .offering(org, provider)
            .await?
            .into_iter()
            .find(|offered| &*offered.id == wanted)
            .ok_or(ConnectionError::NoSuchDestination)?;
        self.filings
            .aim(org, provider, project, Some(&named))
            .await?;
        self.filings.of(org).await
    }

    /* Standing is checked by the two above rather than here, so that aiming reads the list once
    and asks who is asking once. */
    async fn offering(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> Result<Vec<Destination>, ConnectionError> {
        let credential = self
            .connections
            .credential(org, provider)
            .await?
            .ok_or(ConnectionError::NotConnected)?;
        self.destinations.offered(provider, &credential).await
    }

    async fn standing(
        &self,
        token: &SessionToken,
        org: TenantId,
        floor: OrgRole,
    ) -> Result<(), ConnectionError> {
        let who = self.whoami.of(token).await?;
        match who.role_in(org) {
            None => Err(ConnectionError::NotYours),
            Some(role) if !role.at_least(floor) => Err(ConnectionError::NotYoursToChange),
            Some(_) => Ok(()),
        }
    }
}
#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use uuid::Uuid;

    use super::{
        Attempt, Attempts, Authorized, Connecting, Connection, ConnectionError, Connections,
        Destination, Destinations, Filing, Filings, Handshake, Linking, Provider, Routing,
    };
    use crate::WhoamiError;
    use crate::catalogue::ProjectId;
    use crate::clock::UnixSeconds;
    use crate::credential::Email;
    use crate::identity::AccountId;
    use crate::membership::{Membership, Memberships, OrgRole};
    use crate::oidc::FlowError;
    use crate::secret::SessionToken;
    use crate::session::{Session, SessionError, SessionStore};
    use crate::tenant::TenantId;
    use crate::vault::ProviderToken;

    const TOKEN: &str = "ct_ss_live";
    const MINE: u128 = 1;
    const THEIRS: u128 = 2;
    const ME: u128 = 9;

    fn an_org(seed: u128) -> TenantId {
        TenantId::new(Uuid::from_u128(seed))
    }

    fn an_account(seed: u128) -> AccountId {
        AccountId::new(Uuid::from_u128(seed))
    }

    struct OneSession;

    impl SessionStore for OneSession {
        async fn issue(
            &self,
            _account: AccountId,
            _org: Option<TenantId>,
        ) -> Result<SessionToken, SessionError> {
            Ok(SessionToken::from_plaintext(TOKEN))
        }

        async fn resolve(&self, token: &SessionToken) -> Result<Session, SessionError> {
            if token.exposed() == TOKEN {
                Ok(Session {
                    account: an_account(ME),
                    email: Email::try_from("person@example.com").expect("an address"),
                    org: None,
                    expires_at: UnixSeconds::new(0),
                })
            } else {
                Err(SessionError::Unknown)
            }
        }

        async fn revoke(&self, _token: &SessionToken) -> Result<(), SessionError> {
            Ok(())
        }
    }

    /* `None` is somebody who belongs to no organisation at all, which is what the starter of a
    flow looks like after being removed. */
    #[derive(Clone)]
    struct MemberOfMine(Option<OrgRole>);

    impl Memberships for MemberOfMine {
        async fn of(&self, _account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
            Ok(self
                .0
                .map(|role| {
                    vec![Membership {
                        org: an_org(MINE),
                        name: "Acme".into(),
                        role,
                    }]
                })
                .unwrap_or_default())
        }
    }

    #[derive(Debug, Default)]
    struct Held {
        begun: Mutex<Vec<Attempt>>,
        connected: Mutex<Vec<(TenantId, Provider, String, AccountId)>>,
        waiting: Mutex<Option<Attempt>>,
        dropped: Mutex<Vec<(TenantId, Provider)>>,
        there_to_drop: bool,
    }

    impl Held {
        fn waiting_on(attempt: Attempt) -> Self {
            Self {
                waiting: Mutex::new(Some(attempt)),
                ..Self::default()
            }
        }

        fn connected(&self) -> Vec<(TenantId, Provider, String, AccountId)> {
            self.connected.lock().expect("the lock").clone()
        }

        fn begun(&self) -> Vec<Attempt> {
            self.begun.lock().expect("the lock").clone()
        }
    }

    impl Attempts for &Held {
        async fn begin(&self, attempt: &Attempt) -> Result<(), FlowError> {
            self.begun.lock().expect("the lock").push(attempt.clone());
            Ok(())
        }

        async fn take(&self, state: &str) -> Result<Attempt, FlowError> {
            let mut waiting = self.waiting.lock().expect("the lock");
            match waiting.take() {
                Some(attempt) if attempt.state == state => Ok(attempt),
                /* Put back, so a wrong state does not spend somebody elses attempt. */
                Some(attempt) => {
                    *waiting = Some(attempt);
                    Err(FlowError::Unknown)
                }
                None => Err(FlowError::Unknown),
            }
        }
    }

    impl Connections for &Held {
        async fn of(&self, _org: TenantId) -> Result<Vec<Connection>, ConnectionError> {
            Ok(Vec::new())
        }

        async fn connect(
            &self,
            org: TenantId,
            connecting: Connecting<'_>,
        ) -> Result<(), ConnectionError> {
            self.connected.lock().expect("the lock").push((
                org,
                connecting.provider,
                connecting.credential.exposed().to_owned(),
                connecting.by,
            ));
            Ok(())
        }

        async fn credential(
            &self,
            _org: TenantId,
            _provider: Provider,
        ) -> Result<Option<ProviderToken>, ConnectionError> {
            Ok(None)
        }

        async fn disconnect(
            &self,
            org: TenantId,
            provider: Provider,
        ) -> Result<bool, ConnectionError> {
            self.dropped.lock().expect("the lock").push((org, provider));
            Ok(self.there_to_drop)
        }
    }

    struct ALinear;

    impl Handshake for ALinear {
        fn authorize_url(
            &self,
            provider: Provider,
            state: &str,
        ) -> Result<String, ConnectionError> {
            if provider != Provider::Linear {
                return Err(ConnectionError::ProviderUnconfigured);
            }
            Ok(format!("https://linear.app/oauth/authorize?state={state}"))
        }

        async fn finish(
            &self,
            _provider: Provider,
            code: &str,
        ) -> Result<Authorized, ConnectionError> {
            if code == "a-good-code" {
                Ok(Authorized {
                    credential: ProviderToken::new("a-credential"),
                    account_label: "Acme Engineering".into(),
                })
            } else {
                Err(ConnectionError::ProviderRefused)
            }
        }
    }

    fn linking(
        role: Option<OrgRole>,
        held: &Held,
    ) -> Linking<OneSession, MemberOfMine, &Held, &Held, ALinear> {
        Linking::new(OneSession, MemberOfMine(role), held, held, ALinear)
    }

    fn token() -> SessionToken {
        SessionToken::from_plaintext(TOKEN)
    }

    #[tokio::test]
    async fn an_administrator_starts_a_flow_and_is_told_where_to_send_the_browser() {
        let held = Held::default();

        let url = linking(Some(OrgRole::Admin), &held)
            .start(&token(), an_org(MINE), Provider::Linear)
            .await
            .expect("an administrator may connect");

        let begun = held.begun();
        assert_eq!(begun.len(), 1);
        assert_eq!(begun[0].org, an_org(MINE));
        assert_eq!(begun[0].by, an_account(ME));
        assert_eq!(begun[0].provider, Provider::Linear);
        assert!(
            url.contains(&begun[0].state),
            "the state written down is the state sent: {url}"
        );
    }

    /* Two flows started in two tabs must not share a state, or finishing one finishes the other. */
    #[tokio::test]
    async fn two_attempts_never_share_a_state() {
        let held = Held::default();
        let linking = linking(Some(OrgRole::Admin), &held);

        linking
            .start(&token(), an_org(MINE), Provider::Linear)
            .await
            .expect("the first");
        linking
            .start(&token(), an_org(MINE), Provider::Linear)
            .await
            .expect("the second");

        let begun = held.begun();
        assert_ne!(begun[0].state, begun[1].state);
        assert_eq!(begun[0].state.len(), 64, "thirty-two bytes of hex");
    }

    /* A provider this deployment registered no application with leaves no attempt behind, which
    is why the url is built before the row is written: an attempt nobody can finish would sit
    there until it expired. */
    #[tokio::test]
    async fn a_provider_this_deployment_never_registered_writes_no_attempt() {
        let held = Held::default();

        let refused = linking(Some(OrgRole::Admin), &held)
            .start(&token(), an_org(MINE), Provider::Jira)
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::ProviderUnconfigured));
        assert!(held.begun().is_empty());
    }

    #[tokio::test]
    async fn a_member_connects_nothing() {
        let held = Held::default();

        let refused = linking(Some(OrgRole::Member), &held)
            .start(&token(), an_org(MINE), Provider::Linear)
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::NotYoursToChange));
        assert!(held.begun().is_empty());
    }

    #[tokio::test]
    async fn starting_a_flow_for_another_organisation_says_no_such_thing() {
        let held = Held::default();

        let refused = linking(Some(OrgRole::Owner), &held)
            .start(&token(), an_org(THEIRS), Provider::Linear)
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::NotYours));
        assert!(held.begun().is_empty());
    }

    #[tokio::test]
    async fn a_finished_flow_seals_the_credential_against_the_organisation_the_row_names() {
        let attempt = Attempt::fresh(an_org(MINE), an_account(ME), Provider::Linear);
        let state = attempt.state.clone();
        let held = Held::waiting_on(attempt);

        let connection = linking(Some(OrgRole::Admin), &held)
            .finish(Provider::Linear, &state, "a-good-code")
            .await
            .expect("the flow finishes");

        assert_eq!(&*connection.account_label, "Acme Engineering");
        assert_eq!(
            held.connected(),
            vec![(
                an_org(MINE),
                Provider::Linear,
                "a-credential".to_owned(),
                an_account(ME)
            )]
        );
    }

    /* The state is spent on the way in, so a callback replayed — a bookmarked url, a retried
    request — does nothing the second time. */
    #[tokio::test]
    async fn a_callback_replayed_connects_nothing_the_second_time() {
        let attempt = Attempt::fresh(an_org(MINE), an_account(ME), Provider::Linear);
        let state = attempt.state.clone();
        let held = Held::waiting_on(attempt);
        let linking = linking(Some(OrgRole::Admin), &held);
        linking
            .finish(Provider::Linear, &state, "a-good-code")
            .await
            .expect("the first time");

        let refused = linking
            .finish(Provider::Linear, &state, "a-good-code")
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::NoSuchAttempt));
        assert_eq!(held.connected().len(), 1);
    }

    /* A state minted for one provider must not be redeemable at another callback, or a flow
    started for Linear could hand a Linear credential to the Jira row. */
    #[tokio::test]
    async fn a_state_minted_for_one_provider_is_not_redeemable_at_another() {
        let attempt = Attempt::fresh(an_org(MINE), an_account(ME), Provider::Linear);
        let state = attempt.state.clone();
        let held = Held::waiting_on(attempt);

        let refused = linking(Some(OrgRole::Admin), &held)
            .finish(Provider::Jira, &state, "a-good-code")
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::NoSuchAttempt));
        assert!(held.connected().is_empty());
    }

    /* The check the attempt row cannot make for itself. A flow started on Monday and finished on
    Tuesday was started by somebody who may since have been demoted or removed; the row says who
    they were, not what they still are. */
    #[tokio::test]
    async fn a_flow_finished_after_the_starter_lost_the_role_connects_nothing() {
        let attempt = Attempt::fresh(an_org(MINE), an_account(ME), Provider::Linear);
        let state = attempt.state.clone();
        let held = Held::waiting_on(attempt);

        let refused = linking(Some(OrgRole::Member), &held)
            .finish(Provider::Linear, &state, "a-good-code")
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::NotYoursToChange));
        assert!(held.connected().is_empty());
    }

    #[tokio::test]
    async fn a_flow_finished_after_the_starter_left_connects_nothing_either() {
        let attempt = Attempt::fresh(an_org(MINE), an_account(ME), Provider::Linear);
        let state = attempt.state.clone();
        let held = Held::waiting_on(attempt);

        let refused = linking(None, &held)
            .finish(Provider::Linear, &state, "a-good-code")
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::NotYoursToChange));
        assert!(held.connected().is_empty());
    }

    #[tokio::test]
    async fn a_provider_that_refuses_the_code_connects_nothing() {
        let attempt = Attempt::fresh(an_org(MINE), an_account(ME), Provider::Linear);
        let state = attempt.state.clone();
        let held = Held::waiting_on(attempt);

        let refused = linking(Some(OrgRole::Admin), &held)
            .finish(Provider::Linear, &state, "a-stale-code")
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::ProviderRefused));
        assert!(held.connected().is_empty());
    }

    #[tokio::test]
    async fn an_administrator_disconnects_a_provider() {
        let held = Held {
            there_to_drop: true,
            ..Held::default()
        };

        linking(Some(OrgRole::Admin), &held)
            .disconnect(&token(), an_org(MINE), Provider::Linear)
            .await
            .expect("an administrator may disconnect");

        assert_eq!(
            *held.dropped.lock().expect("the lock"),
            vec![(an_org(MINE), Provider::Linear)]
        );
    }

    #[tokio::test]
    async fn disconnecting_what_was_never_connected_says_so() {
        let held = Held::default();

        let refused = linking(Some(OrgRole::Owner), &held)
            .disconnect(&token(), an_org(MINE), Provider::Linear)
            .await;

        assert_eq!(refused.err(), Some(ConnectionError::NotConnected));
    }

    #[test]
    fn every_provider_survives_a_round_trip_through_its_spelling() {
        for provider in Provider::every() {
            assert_eq!(Provider::read(provider.as_str()), Some(provider));
        }
    }

    #[test]
    fn a_spelling_the_schema_does_not_hold_is_refused() {
        assert_eq!(Provider::read("gitlab"), None);
        assert_eq!(Provider::read("Linear"), None);
        assert_eq!(Provider::read(""), None);
    }

    /* The list the screen iterates to ask about the ones that are not connected. A provider
    added to the enum and forgotten here would simply never appear. */
    #[test]
    fn every_provider_is_in_the_list_of_all_of_them() {
        assert_eq!(
            Provider::every()
                .iter()
                .map(|one| one.as_str())
                .collect::<Vec<_>>(),
            vec!["linear", "github", "jira"]
        );
    }

    /* What the provider offers and what has been aimed, in one place, because the point of most
    of these tests is that the second is written from the first. */
    type Choice = (TenantId, Provider, Option<ProjectId>, Option<Destination>);

    #[derive(Debug, Default)]
    struct Aimed {
        offers: Vec<(String, String)>,
        connected: bool,
        chosen: Mutex<Vec<Choice>>,
        asked: Mutex<u32>,
    }

    impl Aimed {
        fn offering(offers: &[(&str, &str)]) -> Self {
            Self {
                offers: offers
                    .iter()
                    .map(|(id, name)| ((*id).to_owned(), (*name).to_owned()))
                    .collect(),
                connected: true,
                ..Self::default()
            }
        }

        fn aimed(&self) -> Vec<Choice> {
            self.chosen.lock().expect("the lock").clone()
        }

        fn asked(&self) -> u32 {
            *self.asked.lock().expect("the lock")
        }
    }

    impl Connections for &Aimed {
        async fn of(&self, _org: TenantId) -> Result<Vec<Connection>, ConnectionError> {
            Ok(Vec::new())
        }

        async fn connect(
            &self,
            _org: TenantId,
            _connecting: Connecting<'_>,
        ) -> Result<(), ConnectionError> {
            Ok(())
        }

        async fn credential(
            &self,
            _org: TenantId,
            _provider: Provider,
        ) -> Result<Option<ProviderToken>, ConnectionError> {
            Ok(self.connected.then(|| ProviderToken::new("a-credential")))
        }

        async fn disconnect(
            &self,
            _org: TenantId,
            _provider: Provider,
        ) -> Result<bool, ConnectionError> {
            Ok(true)
        }
    }

    impl Filings for &Aimed {
        async fn of(&self, _org: TenantId) -> Result<Vec<Routing>, ConnectionError> {
            let aimed = self.chosen.lock().expect("the lock");
            Ok(vec![Routing {
                provider: Provider::Linear,
                default: aimed
                    .iter()
                    .rev()
                    .find(|(_, _, project, _)| project.is_none())
                    .and_then(|(_, _, _, destination)| destination.clone()),
                projects: aimed
                    .iter()
                    .filter_map(|(_, _, project, destination)| {
                        Some(((*project)?, destination.clone()?))
                    })
                    .collect(),
            }])
        }

        async fn aim(
            &self,
            org: TenantId,
            provider: Provider,
            project: Option<ProjectId>,
            destination: Option<&Destination>,
        ) -> Result<(), ConnectionError> {
            self.chosen.lock().expect("the lock").push((
                org,
                provider,
                project,
                destination.cloned(),
            ));
            Ok(())
        }
    }

    impl Destinations for &Aimed {
        async fn offered(
            &self,
            _provider: Provider,
            _credential: &ProviderToken,
        ) -> Result<Vec<Destination>, ConnectionError> {
            *self.asked.lock().expect("the lock") += 1;
            Ok(self
                .offers
                .iter()
                .map(|(id, name)| Destination {
                    id: id.clone().into(),
                    name: name.clone().into(),
                })
                .collect())
        }
    }

    fn filing(
        role: Option<OrgRole>,
        aimed: &Aimed,
    ) -> Filing<OneSession, MemberOfMine, &Aimed, &Aimed, &Aimed> {
        Filing::new(OneSession, MemberOfMine(role), aimed, aimed, aimed)
    }

    fn a_project(seed: u128) -> ProjectId {
        ProjectId::new(Uuid::from_u128(seed))
    }

    #[tokio::test]
    async fn a_member_reads_where_issues_go() {
        let aimed = Aimed::offering(&[("team-1", "Engineering (ENG)")]);

        let routings = filing(Some(OrgRole::Member), &aimed)
            .routings(&token(), an_org(MINE))
            .await
            .expect("the routings");

        assert_eq!(routings.len(), 1);
        assert_eq!(routings[0].provider, Provider::Linear);
        assert_eq!(routings[0].default, None);
    }

    #[tokio::test]
    async fn somebody_outside_the_organisation_reads_nothing() {
        let aimed = Aimed::offering(&[]);

        let refused = filing(Some(OrgRole::Owner), &aimed)
            .routings(&token(), an_org(THEIRS))
            .await
            .expect_err("a refusal");

        assert_eq!(refused, ConnectionError::NotYours);
    }

    #[tokio::test]
    async fn a_member_does_not_choose_where_issues_go() {
        let aimed = Aimed::offering(&[("team-1", "Engineering (ENG)")]);

        let refused = filing(Some(OrgRole::Member), &aimed)
            .aim(
                &token(),
                an_org(MINE),
                Provider::Linear,
                None,
                Some("team-1"),
            )
            .await
            .expect_err("a refusal");

        assert_eq!(refused, ConnectionError::NotYoursToChange);
        assert!(aimed.aimed().is_empty());
    }

    /* The reason the name is not in the request: a screen that labelled team-1 "Billing" would
    otherwise have everybody reading the wrong word off every table afterwards. */
    #[tokio::test]
    async fn the_name_comes_off_the_offered_list_and_not_out_of_the_request() {
        let aimed = Aimed::offering(&[("team-1", "Engineering (ENG)")]);

        filing(Some(OrgRole::Admin), &aimed)
            .aim(
                &token(),
                an_org(MINE),
                Provider::Linear,
                Some(a_project(7)),
                Some("team-1"),
            )
            .await
            .expect("the routings");

        let (org, provider, project, destination) =
            aimed.aimed().pop().expect("something was aimed");
        assert_eq!(org, an_org(MINE));
        assert_eq!(provider, Provider::Linear);
        assert_eq!(project, Some(a_project(7)));
        assert_eq!(
            destination,
            Some(Destination {
                id: "team-1".into(),
                name: "Engineering (ENG)".into(),
            })
        );
    }

    #[tokio::test]
    async fn a_destination_that_is_not_on_offer_is_refused() {
        let aimed = Aimed::offering(&[("team-1", "Engineering (ENG)")]);

        let refused = filing(Some(OrgRole::Admin), &aimed)
            .aim(
                &token(),
                an_org(MINE),
                Provider::Linear,
                None,
                Some("team-archived"),
            )
            .await
            .expect_err("a refusal");

        assert_eq!(refused, ConnectionError::NoSuchDestination);
        assert!(aimed.aimed().is_empty());
    }

    #[tokio::test]
    async fn no_project_means_the_organisations_default() {
        let aimed = Aimed::offering(&[("team-2", "Billing (BIL)")]);

        let routings = filing(Some(OrgRole::Admin), &aimed)
            .aim(
                &token(),
                an_org(MINE),
                Provider::Linear,
                None,
                Some("team-2"),
            )
            .await
            .expect("the routings");

        assert_eq!(
            routings[0].default,
            Some(Destination {
                id: "team-2".into(),
                name: "Billing (BIL)".into(),
            })
        );
        assert!(routings[0].projects.is_empty());
    }

    /* Clearing a choice does not ask the provider anything, which matters on the day the provider
    is down: somebody who wants to stop filing issues somewhere should not be held up by the place
    they are trying to stop filing them. */
    #[tokio::test]
    async fn clearing_a_choice_does_not_call_the_provider() {
        let aimed = Aimed::offering(&[("team-1", "Engineering (ENG)")]);

        filing(Some(OrgRole::Admin), &aimed)
            .aim(
                &token(),
                an_org(MINE),
                Provider::Linear,
                Some(a_project(7)),
                None,
            )
            .await
            .expect("the routings");

        assert_eq!(aimed.asked(), 0);
        assert_eq!(aimed.aimed().pop().expect("a clearing").3, None);
    }

    #[tokio::test]
    async fn nothing_is_on_offer_from_a_provider_nobody_is_connected_to() {
        let aimed = Aimed::default();

        let refused = filing(Some(OrgRole::Admin), &aimed)
            .offered(&token(), an_org(MINE), Provider::Linear)
            .await
            .expect_err("a refusal");

        assert_eq!(refused, ConnectionError::NotConnected);
        assert_eq!(aimed.asked(), 0);
    }
}
