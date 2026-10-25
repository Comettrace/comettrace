use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, COOKIE};
use axum::http::{Method, Request, StatusCode};
use comettrace_api::filing_router;
use comettrace_core::{
    AccountId, Connecting, Connection, ConnectionError, Connections, Destination, Destinations,
    Email, Filing, Filings, Membership, Memberships, OrgRole, ProjectId, Provider, ProviderToken,
    Routing, Session, SessionError, SessionStore, SessionToken, TenantId, UnixSeconds, WhoamiError,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "ct_ss_0123456789";
const MINE: u128 = 1;
const THEIRS: u128 = 2;
const ME: u128 = 9;
const A_PROJECT: u128 = 7;

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

/* One double for all three ports, because the thing being tested is that a choice made through
one route comes back out of another. */
#[derive(Debug, Default)]
struct Kept {
    offers: Vec<(&'static str, &'static str)>,
    connected: bool,
    chosen: Mutex<Vec<(Option<ProjectId>, Option<Destination>)>>,
}

impl Kept {
    fn offering(offers: &[(&'static str, &'static str)]) -> Arc<Self> {
        Arc::new(Self {
            offers: offers.to_vec(),
            connected: true,
            ..Self::default()
        })
    }

    fn unconnected() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

impl Connections for Kept {
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

impl Filings for Kept {
    async fn of(&self, _org: TenantId) -> Result<Vec<Routing>, ConnectionError> {
        let chosen = self.chosen.lock().expect("the lock");
        Ok(vec![Routing {
            provider: Provider::Linear,
            default: chosen
                .iter()
                .rev()
                .find(|(project, _)| project.is_none())
                .and_then(|(_, destination)| destination.clone()),
            projects: chosen
                .iter()
                .filter_map(|(project, destination)| Some(((*project)?, destination.clone()?)))
                .collect(),
        }])
    }

    async fn aim(
        &self,
        _org: TenantId,
        _provider: Provider,
        project: Option<ProjectId>,
        destination: Option<&Destination>,
    ) -> Result<(), ConnectionError> {
        self.chosen
            .lock()
            .expect("the lock")
            .push((project, destination.cloned()));
        Ok(())
    }
}

impl Destinations for Kept {
    async fn offered(
        &self,
        _provider: Provider,
        _credential: &ProviderToken,
    ) -> Result<Vec<Destination>, ConnectionError> {
        Ok(self
            .offers
            .iter()
            .map(|(id, name)| Destination {
                id: (*id).into(),
                name: (*name).into(),
            })
            .collect())
    }
}

type Aiming = Filing<OneSession, MemberOfMine, Arc<Kept>, Arc<Kept>, Arc<Kept>>;

fn asking(method: Method, uri: &str, body: Option<&str>) -> Request<Body> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(COOKIE, format!("ct_session={TOKEN}"))
        .header("x-requested-with", "fetch");
    match body {
        Some(json) => request
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_owned()))
            .expect("a request"),
        None => request.body(Body::empty()).expect("a request"),
    }
}

async fn served(role: OrgRole, kept: &Arc<Kept>, request: Request<Body>) -> (StatusCode, String) {
    let filing: Arc<Aiming> = Arc::new(Filing::new(
        OneSession,
        MemberOfMine(role),
        Arc::clone(kept),
        Arc::clone(kept),
        Arc::clone(kept),
    ));
    let response = filing_router(filing)
        .oneshot(request)
        .await
        .expect("a response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    (status, String::from_utf8(body.to_vec()).expect("text"))
}

fn read(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("json")
}

#[tokio::test]
async fn a_member_reads_where_issues_go() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    let (status, body) = served(
        OrgRole::Member,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/filings", id(MINE)),
            None,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let answered = read(&body);
    assert_eq!(answered["routings"][0]["provider"], "linear");
    assert!(answered["routings"][0]["default"].is_null());
    assert_eq!(
        answered["routings"][0]["projects"]
            .as_array()
            .expect("the projects")
            .len(),
        0
    );
}

#[tokio::test]
async fn another_organisations_filings_are_not_there() {
    let kept = Kept::offering(&[]);

    let (status, _) = served(
        OrgRole::Owner,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/filings", id(THEIRS)),
            None,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_administrator_sees_what_the_provider_offers() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)"), ("team-2", "Billing (BIL)")]);

    let (status, body) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/filings/linear/destinations", id(MINE)),
            None,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let answered = read(&body);
    assert_eq!(answered["destinations"][0]["id"], "team-1");
    assert_eq!(answered["destinations"][1]["name"], "Billing (BIL)");
}

/* The credential is what a listing needs, so a member asking is asking for a call made with a
secret they have no business spending. */
#[tokio::test]
async fn a_member_does_not_see_what_the_provider_offers() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    let (status, _) = served(
        OrgRole::Member,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/filings/linear/destinations", id(MINE)),
            None,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn nothing_is_offered_by_a_provider_nobody_is_connected_to() {
    let kept = Kept::unconnected();

    let (status, _) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::GET,
            &format!("/api/orgs/{}/filings/linear/destinations", id(MINE)),
            None,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_administrator_chooses_the_organisations_default() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    let (status, body) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::PUT,
            &format!("/api/orgs/{}/filings/linear", id(MINE)),
            Some(r#"{"destination":"team-1"}"#),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let answered = read(&body);
    assert_eq!(answered["routings"][0]["default"]["id"], "team-1");
    assert_eq!(
        answered["routings"][0]["default"]["name"],
        "Engineering (ENG)"
    );
}

#[tokio::test]
async fn an_administrator_points_one_project_somewhere_else() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)"), ("team-2", "Billing (BIL)")]);

    let (status, body) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::PUT,
            &format!("/api/orgs/{}/filings/linear", id(MINE)),
            Some(&format!(
                r#"{{"project":"{}","destination":"team-2"}}"#,
                id(A_PROJECT)
            )),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let answered = read(&body);
    assert!(answered["routings"][0]["default"].is_null());
    assert_eq!(
        answered["routings"][0]["projects"][0]["project"],
        id(A_PROJECT)
    );
    assert_eq!(
        answered["routings"][0]["projects"][0]["destination"]["name"],
        "Billing (BIL)"
    );
}

/* The screen sends the id; the name is read off the offered list. A request carrying both would
let a stale screen write a word nobody chose. */
#[tokio::test]
async fn a_name_in_the_request_is_not_the_name_that_is_written() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    let (status, body) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::PUT,
            &format!("/api/orgs/{}/filings/linear", id(MINE)),
            Some(r#"{"destination":"team-1","name":"Payroll"}"#),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        read(&body)["routings"][0]["default"]["name"],
        "Engineering (ENG)"
    );
}

#[tokio::test]
async fn somewhere_the_provider_does_not_offer_is_refused() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    let (status, _) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::PUT,
            &format!("/api/orgs/{}/filings/linear", id(MINE)),
            Some(r#"{"destination":"team-archived"}"#),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(kept.chosen.lock().expect("the lock").is_empty());
}

#[tokio::test]
async fn a_member_does_not_choose_where_issues_go() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    let (status, _) = served(
        OrgRole::Member,
        &kept,
        asking(
            Method::PUT,
            &format!("/api/orgs/{}/filings/linear", id(MINE)),
            Some(r#"{"destination":"team-1"}"#),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(kept.chosen.lock().expect("the lock").is_empty());
}

#[tokio::test]
async fn a_project_can_be_sent_back_to_following_the_default() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    let (status, body) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::PUT,
            &format!("/api/orgs/{}/filings/linear", id(MINE)),
            Some(&format!(r#"{{"project":"{}"}}"#, id(A_PROJECT))),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        read(&body)["routings"][0]["projects"]
            .as_array()
            .expect("the projects")
            .len(),
        0
    );
}

#[tokio::test]
async fn a_provider_this_application_does_not_know_is_not_there() {
    let kept = Kept::offering(&[]);

    let (status, _) = served(
        OrgRole::Admin,
        &kept,
        asking(
            Method::PUT,
            &format!("/api/orgs/{}/filings/trello", id(MINE)),
            Some(r#"{"destination":"team-1"}"#),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/* Nothing on these routes carries a credential, and this is the test that keeps it that way. */
#[tokio::test]
async fn no_answer_here_carries_a_secret() {
    let kept = Kept::offering(&[("team-1", "Engineering (ENG)")]);

    for uri in [
        format!("/api/orgs/{}/filings", id(MINE)),
        format!("/api/orgs/{}/filings/linear/destinations", id(MINE)),
    ] {
        let (status, body) = served(OrgRole::Admin, &kept, asking(Method::GET, &uri, None)).await;
        assert_eq!(status, StatusCode::OK);
        for word in ["credential", "token", "secret", "nonce"] {
            assert!(!body.contains(word), "{uri} said {word}");
        }
    }
}
