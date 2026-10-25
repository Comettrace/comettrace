use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::header::{COOKIE, LOCATION};
use axum::http::{Method, Request, StatusCode};
use comettrace_api::connection_router;
use comettrace_core::{
    AccountId, Attempt, Attempts, Authorized, Connecting, Connection, ConnectionError, Connections,
    Email, FlowError, Handshake, Linking, Membership, Memberships, OrgRole, Provider,
    ProviderToken, Session, SessionError, SessionStore, SessionToken, TenantId, UnixSeconds,
    WhoamiError,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "ct_ss_0123456789";
const MINE: u128 = 1;
const THEIRS: u128 = 2;
const ME: u128 = 9;

fn id(seed: u128) -> String {
    Uuid::from_u128(seed).to_string()
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
                account: AccountId::new(Uuid::from_u128(ME)),
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

#[derive(Clone)]
struct MemberOfMine(OrgRole);

impl Memberships for MemberOfMine {
    async fn of(&self, _account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
        Ok(vec![Membership {
            org: TenantId::new(Uuid::from_u128(MINE)),
            name: "Acme".into(),
            role: self.0,
        }])
    }
}

#[derive(Debug, Default)]
struct Kept {
    connected: Mutex<Vec<(TenantId, Provider, String)>>,
    waiting: Mutex<Option<Attempt>>,
    listed: Mutex<Vec<Connection>>,
    there_to_drop: bool,
}

impl Kept {
    fn holding(listed: Vec<Connection>) -> Arc<Self> {
        Arc::new(Self {
            listed: Mutex::new(listed),
            ..Self::default()
        })
    }

    fn waiting_on(attempt: Attempt) -> Arc<Self> {
        Arc::new(Self {
            waiting: Mutex::new(Some(attempt)),
            ..Self::default()
        })
    }

    fn connectable() -> Arc<Self> {
        Arc::new(Self {
            there_to_drop: true,
            ..Self::default()
        })
    }

    fn connected(&self) -> Vec<(TenantId, Provider, String)> {
        self.connected.lock().expect("the lock").clone()
    }
}

impl Attempts for Kept {
    async fn begin(&self, attempt: &Attempt) -> Result<(), FlowError> {
        *self.waiting.lock().expect("the lock") = Some(attempt.clone());
        Ok(())
    }

    async fn take(&self, state: &str) -> Result<Attempt, FlowError> {
        let mut waiting = self.waiting.lock().expect("the lock");
        match waiting.take() {
            Some(attempt) if attempt.state == state => Ok(attempt),
            Some(attempt) => {
                *waiting = Some(attempt);
                Err(FlowError::Unknown)
            }
            None => Err(FlowError::Unknown),
        }
    }
}

impl Connections for Kept {
    async fn of(&self, _org: TenantId) -> Result<Vec<Connection>, ConnectionError> {
        Ok(self.listed.lock().expect("the lock").clone())
    }

    async fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> Result<(), ConnectionError> {
        self.connected.lock().expect("the lock").push((
            org,
            connecting.provider,
            connecting.account_label.to_owned(),
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
        _org: TenantId,
        _provider: Provider,
    ) -> Result<bool, ConnectionError> {
        Ok(self.there_to_drop)
    }
}

struct ALinear;

impl Handshake for ALinear {
    fn authorize_url(&self, provider: Provider, state: &str) -> Result<String, ConnectionError> {
        if provider == Provider::Linear {
            Ok(format!("https://linear.app/oauth/authorize?state={state}"))
        } else {
            Err(ConnectionError::ProviderUnconfigured)
        }
    }

    async fn finish(&self, _provider: Provider, code: &str) -> Result<Authorized, ConnectionError> {
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

type Linked = Linking<OneSession, MemberOfMine, Arc<Kept>, Arc<Kept>, ALinear>;

fn asking(method: Method, uri: &str, session: Option<&str>, scripted: bool) -> Request<Body> {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = session {
        request = request.header(COOKIE, format!("ct_session={token}"));
    }
    if scripted {
        request = request.header("x-requested-with", "fetch");
    }
    request.body(Body::empty()).expect("a request")
}

async fn served(
    role: OrgRole,
    kept: &Arc<Kept>,
    request: Request<Body>,
) -> (StatusCode, String, Option<String>) {
    let linking: Arc<Linked> = Arc::new(Linking::new(
        OneSession,
        MemberOfMine(role),
        Arc::clone(kept),
        Arc::clone(kept),
        ALinear,
    ));
    let response = connection_router(linking)
        .oneshot(request)
        .await
        .expect("a response");
    let status = response.status();
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    (
        status,
        String::from_utf8(body.to_vec()).expect("text"),
        location,
    )
}

fn read(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("json")
}

fn a_connection() -> Connection {
    Connection {
        provider: Provider::Linear,
        account_label: "Acme Engineering".into(),
        connected_at_unix_nanos: 1_700_000_000_123_456_789,
        connected_by: Some(AccountId::new(Uuid::from_u128(ME))),
    }
}

#[tokio::test]
async fn a_member_reads_what_its_organisation_is_connected_to() {
    let kept = Kept::holding(vec![a_connection()]);

    let (status, body, _) = served(
        OrgRole::Member,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/integrations", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let linked = &read(&body)["connections"][0];
    assert_eq!(linked["provider"], "linear");
    assert_eq!(linked["account_label"], "Acme Engineering");
    assert_eq!(linked["connected_by"], id(ME));
    assert_eq!(linked["connected_at_ms"], 1_700_000_000_123u64);
}

/* The answer a connection card is drawn from must not be able to carry the credential, whatever
anybody later adds to the row. */
#[tokio::test]
async fn no_credential_reaches_the_list() {
    let kept = Kept::holding(vec![a_connection()]);

    let (_, body, _) = served(
        OrgRole::Member,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/integrations", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    for secret in ["credential", "token", "secret", "nonce"] {
        assert!(!body.contains(secret), "{body}");
    }
}

/* The authorize url is answered rather than redirected to, because a fetch that followed a
redirect across an origin would be stopped by the browser and would take nobody anywhere. */
#[tokio::test]
async fn starting_a_flow_answers_the_url_rather_than_a_redirect() {
    let kept = Kept::connectable();

    let (status, body, location) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::POST,
            &format!("/api/orgs/{}/integrations/linear/start", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(location, None, "a fetch cannot follow this anywhere");
    let url = read(&body)["url"].as_str().expect("a url").to_owned();
    assert!(
        url.starts_with("https://linear.app/oauth/authorize"),
        "{url}"
    );
}

#[tokio::test]
async fn a_member_starts_no_flow() {
    let kept = Kept::connectable();

    let (status, _, _) = served(
        OrgRole::Member,
        &kept,
        asking(
            Method::POST,
            &format!("/api/orgs/{}/integrations/linear/start", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_provider_this_deployment_never_registered_is_a_conflict() {
    let kept = Kept::connectable();

    let (status, body, _) = served(
        OrgRole::Owner,
        &kept,
        asking(
            Method::POST,
            &format!("/api/orgs/{}/integrations/jira/start", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("registered no application"), "{body}");
}

#[tokio::test]
async fn a_provider_nothing_in_this_system_knows_is_not_found() {
    let kept = Kept::connectable();

    let (status, _, _) = served(
        OrgRole::Owner,
        &kept,
        asking(
            Method::POST,
            &format!("/api/orgs/{}/integrations/gitlab/start", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_organisation_this_session_does_not_belong_to_is_not_found() {
    let kept = Kept::connectable();

    let (status, _, _) = served(
        OrgRole::Owner,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/integrations", id(THEIRS)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "telling a stranger an organisation exists is itself a leak"
    );
}

#[tokio::test]
async fn an_administrator_disconnects_a_provider() {
    let kept = Kept::connectable();

    let (status, _, _) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::DELETE,
            &format!("/api/orgs/{}/integrations/linear", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn disconnecting_what_was_never_connected_is_not_found() {
    let kept = Kept::holding(Vec::new());

    let (status, _, _) = served(
        OrgRole::Owner,
        &kept,
        asking(
            Method::DELETE,
            &format!("/api/orgs/{}/integrations/linear", id(MINE)),
            Some(TOKEN),
            true,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_request_from_another_origin_never_reaches_the_connections() {
    let kept = Kept::holding(vec![a_connection()]);

    let (status, _, _) = served(
        OrgRole::Owner,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/integrations", id(MINE)),
            Some(TOKEN),
            false,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/* The callback is the one route here that cannot ask for the header or the cookie: the browser
arrives on a top-level navigation from the provider. The state stands in their place, so it has
to work without either. */
#[tokio::test]
async fn the_callback_needs_neither_a_session_nor_the_header() {
    let attempt = Attempt::fresh(
        TenantId::new(Uuid::from_u128(MINE)),
        AccountId::new(Uuid::from_u128(ME)),
        Provider::Linear,
    );
    let state = attempt.state.clone();
    let kept = Kept::waiting_on(attempt);

    let (status, _, location) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::GET,
            &format!("/api/integrations/linear/callback?code=a-good-code&state={state}"),
            None,
            false,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some("/settings/integrations?connected=linear")
    );
    assert_eq!(
        kept.connected(),
        vec![(
            TenantId::new(Uuid::from_u128(MINE)),
            Provider::Linear,
            "Acme Engineering".to_owned()
        )]
    );
}

/* Win or lose it is a redirect, because the thing reading it is a browser. What went wrong
travels as a word, never as the error text — the query string ends up in a history file. */
#[tokio::test]
async fn a_state_nobody_is_waiting_on_sends_the_browser_back_saying_expired() {
    let kept = Kept::holding(Vec::new());

    let (status, _, location) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::GET,
            "/api/integrations/linear/callback?code=a-good-code&state=invented",
            None,
            false,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some("/settings/integrations?failed=expired")
    );
    assert!(kept.connected().is_empty());
}

#[tokio::test]
async fn consent_that_was_not_given_sends_the_browser_back_saying_declined() {
    let kept = Kept::holding(Vec::new());

    let (status, _, location) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::GET,
            "/api/integrations/linear/callback?error=access_denied&state=whatever",
            None,
            false,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some("/settings/integrations?failed=declined")
    );
}

#[tokio::test]
async fn a_callback_carrying_neither_a_code_nor_a_state_says_incomplete() {
    let kept = Kept::holding(Vec::new());

    let (_, _, location) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::GET,
            "/api/integrations/linear/callback",
            None,
            false,
        ),
    )
    .await;

    assert_eq!(
        location.as_deref(),
        Some("/settings/integrations?failed=incomplete")
    );
}

#[tokio::test]
async fn a_code_the_provider_refuses_sends_the_browser_back_saying_refused() {
    let attempt = Attempt::fresh(
        TenantId::new(Uuid::from_u128(MINE)),
        AccountId::new(Uuid::from_u128(ME)),
        Provider::Linear,
    );
    let state = attempt.state.clone();
    let kept = Kept::waiting_on(attempt);

    let (_, _, location) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::GET,
            &format!("/api/integrations/linear/callback?code=a-stale-code&state={state}"),
            None,
            false,
        ),
    )
    .await;

    assert_eq!(
        location.as_deref(),
        Some("/settings/integrations?failed=refused")
    );
    assert!(kept.connected().is_empty());
}

/* The role is read again on the way back, and a starter who has since been demoted gets the
browser sent back rather than a credential written. */
#[tokio::test]
async fn a_starter_who_lost_the_role_meanwhile_connects_nothing() {
    let attempt = Attempt::fresh(
        TenantId::new(Uuid::from_u128(MINE)),
        AccountId::new(Uuid::from_u128(ME)),
        Provider::Linear,
    );
    let state = attempt.state.clone();
    let kept = Kept::waiting_on(attempt);

    let (_, _, location) = served(
        OrgRole::Member,
        &kept,
        asking(
            Method::GET,
            &format!("/api/integrations/linear/callback?code=a-good-code&state={state}"),
            None,
            false,
        ),
    )
    .await;

    assert_eq!(
        location.as_deref(),
        Some("/settings/integrations?failed=forbidden")
    );
    assert!(kept.connected().is_empty());
}
