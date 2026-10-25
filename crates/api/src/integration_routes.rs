use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post};
use comettrace_core::{
    AccountId, Attempts, Connection, ConnectionError, Connections, Handshake, Linking, Memberships,
    Provider, SessionStore, TenantId, Whoami, WhoamiError,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::explore_routes::{NotCarried, carried};

/* What the deployment is wired up to, and nothing about how. Every field here is either a
boolean or a setting an operator could read off a dashboard — the site a Jira lives at, the
repository an issue would be opened in — and not one of them is a secret. A token is never
anywhere near this route: it is read out of the configuration where it was set, and the only
thing anybody is told about it is that it is there.

Which is also why this is read only. The tokens live in the deployment's configuration file, so
there is nothing on this screen for an administrator to change; the screen's job is to say what
is wired up and name the setting that wired it. */
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Wired {
    pub linear: bool,
    pub github: Option<Repository>,
    pub jira: Option<Site>,
    pub mail: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repository {
    pub repository: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Site {
    pub base_url: String,
    pub project_key: String,
}

#[derive(Debug)]
struct Reading<S, G> {
    whoami: Arc<Whoami<S, G>>,
    wired: Wired,
}

/* Behind a session, unlike `/auth/methods`, which has to answer before anybody has signed in.
What a deployment is plugged into is nobody's business until they are in. */
pub fn integrations_router<S, G>(whoami: Arc<Whoami<S, G>>, wired: Wired) -> Router
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
{
    Router::new()
        .route("/api/integrations", get(wired_up::<S, G>))
        .with_state(Arc::new(Reading { whoami, wired }))
}

async fn wired_up<S, G>(
    State(reading): State<Arc<Reading<S, G>>>,
    headers: HeaderMap,
) -> Result<Json<Wired>, IntegrationRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    reading.whoami.of(&token).await?;
    Ok(Json(reading.wired.clone()))
}

#[derive(Debug, thiserror::Error)]
pub enum IntegrationRefused {
    #[error(transparent)]
    Reaching(#[from] NotCarried),
    #[error("the integrations could not be read")]
    Unavailable,
}

impl From<WhoamiError> for IntegrationRefused {
    fn from(error: WhoamiError) -> Self {
        match error {
            WhoamiError::NoSession => Self::Reaching(NotCarried::NoSession),
            WhoamiError::StoreUnreachable => Self::Unavailable,
        }
    }
}

impl IntoResponse for IntegrationRefused {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Reaching(refused) => return refused.into_response(),
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "the integrations could not be served");
        }
        (status, self.to_string()).into_response()
    }
}

const NANOS_PER_MILLI: u64 = 1_000_000;

/* Where the browser is sent after a handshake, win or lose. A constant rather than a setting:
it is a path of the application this server serves, not a thing a deployment chooses, and the
screen it names is the one the Connect button was pressed on. */
const BACK_TO: &str = "/settings/integrations";

#[derive(Debug, Serialize)]
pub struct Connected {
    connections: Vec<Linked>,
}

/* No secret, by construction: the store does not even select the sealed columns for this. */
#[derive(Debug, Serialize)]
struct Linked {
    provider: &'static str,
    account_label: String,
    connected_at_ms: u64,
    connected_by: Option<Uuid>,
}

impl From<Connection> for Linked {
    fn from(connection: Connection) -> Self {
        Self {
            provider: connection.provider.as_str(),
            account_label: connection.account_label.into_string(),
            connected_at_ms: connection.connected_at_unix_nanos / NANOS_PER_MILLI,
            connected_by: connection.connected_by.map(AccountId::as_uuid),
        }
    }
}

/* The authorize url, for the browser to go to itself. Not a redirect: this is answered to a
fetch, and a fetch that followed a redirect to linear.app would be stopped by the browser for
crossing an origin and would have taken nobody anywhere. */
#[derive(Debug, Serialize)]
pub struct Starting {
    url: String,
}

#[derive(Debug, Deserialize)]
pub struct CameBack {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
    /* A provider that was refused consent says so here instead of sending a code. */
    #[serde(default)]
    error: Option<String>,
}

/* The state every route here carries. An alias because the extractor would otherwise spell five
type parameters out four times over, and the shape is the same every time. */
type Joined<S, G, C, A, H> = Arc<Linking<S, G, C, A, H>>;

pub fn connection_router<S, G, C, A, H>(linking: Joined<S, G, C, A, H>) -> Router
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Clone + Send + Sync + 'static,
    C: Connections + Send + Sync + 'static,
    A: Attempts + Send + Sync + 'static,
    H: Handshake + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/orgs/{org}/integrations",
            get(connections::<S, G, C, A, H>),
        )
        .route(
            "/api/orgs/{org}/integrations/{provider}",
            delete(disconnect::<S, G, C, A, H>),
        )
        .route(
            "/api/orgs/{org}/integrations/{provider}/start",
            post(start::<S, G, C, A, H>),
        )
        /* The one route here with no session and no x-requested-with. It cannot have either: the
        browser arrives on a top-level navigation from the provider, which carries no header this
        application set and may not carry the session cookie at all. What stands in their place is
        the state — minted here, written down here, and spent on the way in. */
        .route(
            "/api/integrations/{provider}/callback",
            get(callback::<S, G, C, A, H>),
        )
        .with_state(linking)
}

async fn connections<S, G, C, A, H>(
    State(linking): State<Joined<S, G, C, A, H>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Connected>, ConnectionRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Clone + Send + Sync + 'static,
    C: Connections + Send + Sync + 'static,
    A: Attempts + Send + Sync + 'static,
    H: Handshake + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let linked = linking.connections(&token, TenantId::new(org)).await?;
    Ok(Json(Connected {
        connections: linked.into_iter().map(Linked::from).collect(),
    }))
}

async fn start<S, G, C, A, H>(
    State(linking): State<Joined<S, G, C, A, H>>,
    Path((org, provider)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Result<Json<Starting>, ConnectionRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Clone + Send + Sync + 'static,
    C: Connections + Send + Sync + 'static,
    A: Attempts + Send + Sync + 'static,
    H: Handshake + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let provider = named(&provider)?;
    let url = linking.start(&token, TenantId::new(org), provider).await?;
    tracing::info!(org = %org, provider = %provider, "a connect flow was started");
    Ok(Json(Starting { url }))
}

async fn disconnect<S, G, C, A, H>(
    State(linking): State<Joined<S, G, C, A, H>>,
    Path((org, provider)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, ConnectionRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Clone + Send + Sync + 'static,
    C: Connections + Send + Sync + 'static,
    A: Attempts + Send + Sync + 'static,
    H: Handshake + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let provider = named(&provider)?;
    linking
        .disconnect(&token, TenantId::new(org), provider)
        .await?;
    tracing::info!(org = %org, provider = %provider, "a provider was disconnected");
    Ok(StatusCode::NO_CONTENT)
}

/* Always a redirect, win or lose, because the thing reading this answer is a browser that a
provider sent here — not code that could read a status. What went wrong travels as a word the
screen turns into a sentence, never as the error text: the query string ends up in a history
file, and these errors name the deployment's own state. */
async fn callback<S, G, C, A, H>(
    State(linking): State<Joined<S, G, C, A, H>>,
    Path(provider): Path<String>,
    Query(came): Query<CameBack>,
) -> Response
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Clone + Send + Sync + 'static,
    C: Connections + Send + Sync + 'static,
    A: Attempts + Send + Sync + 'static,
    H: Handshake + Send + Sync + 'static,
{
    let Ok(provider) = Provider::read(&provider).ok_or(()) else {
        return back("unknown");
    };
    if came.error.is_some() {
        tracing::info!(provider = %provider, "consent was not given");
        return back("declined");
    }
    let (Some(state), Some(code)) = (came.state, came.code) else {
        return back("incomplete");
    };

    match linking.finish(provider, &state, &code).await {
        Ok(connection) => {
            tracing::info!(provider = %provider, "a provider was connected");
            back_connected(&connection.provider.to_string())
        }
        Err(refused) => {
            tracing::warn!(provider = %provider, error = %refused, "a connect flow did not finish");
            back(why(refused))
        }
    }
}

/* Exhaustive on purpose: a variant added later should make this fail to compile rather than
quietly become "unavailable" on a screen somebody is reading for an explanation. */
fn why(refused: ConnectionError) -> &'static str {
    match refused {
        ConnectionError::NoSuchAttempt => "expired",
        ConnectionError::NotYours | ConnectionError::NotYoursToChange => "forbidden",
        ConnectionError::NoKey => "unkeyed",
        ConnectionError::Unopenable => "unopenable",
        ConnectionError::ProviderUnconfigured => "unconfigured",
        ConnectionError::ProviderRefused => "refused",
        ConnectionError::ProviderUnreachable => "unreachable",
        ConnectionError::NoSession
        | ConnectionError::NotConnected
        | ConnectionError::StoreUnreachable => "unavailable",
    }
}

fn back(failed: &str) -> Response {
    Redirect::to(&format!("{BACK_TO}?failed={failed}")).into_response()
}

fn back_connected(provider: &str) -> Response {
    Redirect::to(&format!("{BACK_TO}?connected={provider}")).into_response()
}

fn named(spelling: &str) -> Result<Provider, ConnectionRefused> {
    Provider::read(spelling).ok_or(ConnectionRefused::NoSuchProvider)
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionRefused {
    #[error(transparent)]
    Reaching(#[from] NotCarried),
    #[error("no such organisation")]
    NoSuchOrg,
    #[error("no such provider")]
    NoSuchProvider,
    #[error("connecting a provider is an administrator's to do")]
    NotAnAdministrator,
    #[error("nothing is connected to that provider")]
    NotConnected,
    #[error("this deployment has registered no application with that provider")]
    Unconfigured,
    /* Both of these are the deployment's own state rather than anything the request did, and both
    are worth their own answer: one asks an operator for a key, the other asks an administrator to
    connect again. */
    #[error("this deployment holds no key to seal a connection with")]
    NoKey,
    #[error("this connection does not open under this deployment's key")]
    Unopenable,
    #[error("the provider refused the handshake")]
    ProviderRefused,
    #[error("the provider could not be reached")]
    ProviderUnreachable,
    #[error("the connections could not be read")]
    Unavailable,
}

/* A session with no membership of the organisation is told there is none, so the ids of other
organisations stay unguessable. Everybody past that point already knows it exists. */
impl From<ConnectionError> for ConnectionRefused {
    fn from(error: ConnectionError) -> Self {
        match error {
            ConnectionError::NoSession => Self::Reaching(NotCarried::NoSession),
            ConnectionError::NotYours => Self::NoSuchOrg,
            ConnectionError::NotYoursToChange => Self::NotAnAdministrator,
            /* An attempt nobody is waiting on cannot arrive through this conversion — only the
            callback finishes a flow, and it answers with a redirect rather than a refusal — but
            it reads the same way if it ever does. */
            ConnectionError::NotConnected | ConnectionError::NoSuchAttempt => Self::NotConnected,
            ConnectionError::ProviderUnconfigured => Self::Unconfigured,
            ConnectionError::NoKey => Self::NoKey,
            ConnectionError::Unopenable => Self::Unopenable,
            ConnectionError::ProviderRefused => Self::ProviderRefused,
            ConnectionError::ProviderUnreachable => Self::ProviderUnreachable,
            ConnectionError::StoreUnreachable => Self::Unavailable,
        }
    }
}

impl IntoResponse for ConnectionRefused {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Reaching(refused) => return refused.into_response(),
            Self::NoSuchOrg | Self::NoSuchProvider | Self::NotConnected => StatusCode::NOT_FOUND,
            Self::NotAnAdministrator => StatusCode::FORBIDDEN,
            /* Neither is the request's fault: one wants a setting, the other wants reconnecting,
            and both are things an administrator reading the screen can act on. */
            Self::Unconfigured | Self::NoKey | Self::Unopenable => StatusCode::CONFLICT,
            Self::ProviderRefused => StatusCode::BAD_GATEWAY,
            Self::ProviderUnreachable | Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "the connections could not be served");
        }
        (status, self.to_string()).into_response()
    }
}
