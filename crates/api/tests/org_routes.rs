use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::header::COOKIE;
use axum::http::{Method, Request, StatusCode};
use comettrace_api::org_router;
use comettrace_core::{
    AccountId, Membership, Memberships, OrgName, OrgRole, OrgSettings, Organisation, Orgs,
    ReadError, Session, SessionError, SessionStore, SessionToken, TenantId, UnixSeconds, Whoami,
    WhoamiError,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "ct_ss_0123456789";
const MINE: u128 = 1;
const THEIRS: u128 = 2;
const FOUNDED: u64 = 1_700_000_000_123_456_789;

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
                account: AccountId::new(Uuid::from_u128(9)),
                email: "person@example.com".try_into().expect("an address"),
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

#[derive(Debug)]
struct Kept(Mutex<String>);

impl Orgs for Kept {
    async fn only(&self, org: TenantId) -> Result<Option<Organisation>, ReadError> {
        Ok(Some(Organisation {
            id: org,
            name: self.0.lock().expect("the lock").clone().into(),
            plan: "Self-hosted".into(),
            created_at_unix_nanos: FOUNDED,
        }))
    }

    async fn rename(&self, _org: TenantId, name: &OrgName) -> Result<(), ReadError> {
        *self.0.lock().expect("the lock") = name.as_str().to_owned();
        Ok(())
    }
}

fn org(seed: u128) -> String {
    Uuid::from_u128(seed).to_string()
}

fn session() -> String {
    format!("ct_session={TOKEN}")
}

fn asking(method: Method, seed: u128, body: Option<&str>, scripted: bool) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/api/orgs/{}", org(seed)))
        .header(COOKIE, session());
    if scripted {
        request = request.header("x-requested-with", "fetch");
    }
    if body.is_some() {
        request = request.header("content-type", "application/json");
    }
    request
        .body(body.map_or_else(Body::empty, |json| Body::from(json.to_owned())))
        .expect("a request")
}

async fn served(role: OrgRole, kept: &Arc<Kept>, request: Request<Body>) -> (StatusCode, String) {
    let settings = Arc::new(OrgSettings::new(
        Whoami::new(OneSession, MemberOfMine(role)),
        Arc::clone(kept),
    ));
    let response = org_router(settings)
        .oneshot(request)
        .await
        .expect("a response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    (status, String::from_utf8(body.to_vec()).expect("text"))
}

fn holding(name: &str) -> Arc<Kept> {
    Arc::new(Kept(Mutex::new(name.to_owned())))
}

fn renaming(name: &str) -> String {
    serde_json::json!({ "name": name }).to_string()
}

#[tokio::test]
async fn a_member_reads_the_card_of_its_own_organisation() {
    let kept = holding("Acme");

    let (status, body) = served(
        OrgRole::Member,
        &kept,
        asking(Method::GET, MINE, None, true),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let card: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(card["id"], org(MINE));
    assert_eq!(card["name"], "Acme");
    assert_eq!(card["plan"], "Self-hosted");
    assert_eq!(card["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        card["created_at_ms"], 1_700_000_000_123u64,
        "a time a browser can hold without losing digits"
    );
}

#[tokio::test]
async fn a_request_from_another_origin_never_reaches_the_organisation() {
    let kept = holding("Acme");

    let (status, _) = served(
        OrgRole::Owner,
        &kept,
        asking(Method::GET, MINE, None, false),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_organisation_this_session_does_not_belong_to_is_not_found() {
    let kept = holding("Acme");

    let (status, _) = served(
        OrgRole::Owner,
        &kept,
        asking(Method::GET, THEIRS, None, true),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "telling a stranger an organisation exists is itself a leak"
    );
}

#[tokio::test]
async fn an_administrator_renames_it_and_is_answered_with_the_card() {
    let kept = holding("Acme");

    let (status, body) = served(
        OrgRole::Admin,
        &kept,
        asking(Method::PATCH, MINE, Some(&renaming("Acme Payments")), true),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let card: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(card["name"], "Acme Payments");
    assert_eq!(*kept.0.lock().expect("the lock"), "Acme Payments");
}

/* A member can see the organisation, so hiding it behind a 404 would only puzzle it. The one
thing it cannot do is change it, and that is what it is told. */
#[tokio::test]
async fn a_member_is_told_plainly_that_renaming_is_not_its_to_do() {
    let kept = holding("Acme");

    let (status, _) = served(
        OrgRole::Member,
        &kept,
        asking(Method::PATCH, MINE, Some(&renaming("Mine Now")), true),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(*kept.0.lock().expect("the lock"), "Acme");
}

#[tokio::test]
async fn a_blank_name_is_refused_with_the_rule_that_refused_it() {
    let kept = holding("Acme");

    let (status, body) = served(
        OrgRole::Owner,
        &kept,
        asking(Method::PATCH, MINE, Some(&renaming("   ")), true),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("needs a name"), "{body}");
    assert_eq!(*kept.0.lock().expect("the lock"), "Acme");
}

#[tokio::test]
async fn a_name_is_kept_without_the_space_around_it() {
    let kept = holding("Acme");

    let (status, body) = served(
        OrgRole::Owner,
        &kept,
        asking(Method::PATCH, MINE, Some(&renaming("  Acme  ")), true),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let card: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(card["name"], "Acme");
}
