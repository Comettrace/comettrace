use std::sync::Arc;

use axum::body::Body;
use axum::http::header::COOKIE;
use axum::http::{Request, StatusCode};
use comettrace_api::me_router;
use comettrace_core::{
    AccountId, Membership, Memberships, OrgRole, Session, SessionError, SessionStore, SessionToken,
    TenantId, UnixSeconds, Whoami, WhoamiError,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "ct_ss_0123456789";
const ACCOUNT: u128 = 7;
const ACTIVE: u128 = 11;
const OTHER: u128 = 12;

struct OneSession {
    org: Option<TenantId>,
}

impl SessionStore for OneSession {
    async fn issue(
        &self,
        account: AccountId,
        org: Option<TenantId>,
    ) -> Result<SessionToken, SessionError> {
        let _ = (account, org);
        Ok(SessionToken::from_plaintext(TOKEN))
    }

    async fn resolve(&self, token: &SessionToken) -> Result<Session, SessionError> {
        if token.exposed() == TOKEN {
            Ok(Session {
                account: AccountId::new(Uuid::from_u128(ACCOUNT)),
                email: an_email(),
                org: self.org,
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

struct TwoOrgs;

impl Memberships for TwoOrgs {
    async fn of(&self, account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
        if account != AccountId::new(Uuid::from_u128(ACCOUNT)) {
            return Ok(Vec::new());
        }
        Ok(vec![
            Membership {
                org: TenantId::new(Uuid::from_u128(ACTIVE)),
                name: "Acme".into(),
                role: OrgRole::Owner,
            },
            Membership {
                org: TenantId::new(Uuid::from_u128(OTHER)),
                name: "Globex".into(),
                role: OrgRole::Member,
            },
        ])
    }
}

struct NoControlPlane;

impl Memberships for NoControlPlane {
    async fn of(&self, _account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
        Err(WhoamiError::StoreUnreachable)
    }
}

fn asking(cookie: Option<&str>, scripted: bool) -> Request<Body> {
    let mut request = Request::builder().uri("/auth/me");
    if scripted {
        request = request.header("x-requested-with", "fetch");
    }
    if let Some(cookie) = cookie {
        request = request.header(COOKIE, cookie);
    }
    request.body(Body::empty()).expect("a request")
}

async fn served<G>(memberships: G, request: Request<Body>) -> (StatusCode, String)
where
    G: Memberships + Send + Sync + 'static,
{
    let whoami = Arc::new(Whoami::new(
        OneSession {
            org: Some(TenantId::new(Uuid::from_u128(ACTIVE))),
        },
        memberships,
    ));
    let response = me_router(whoami)
        .oneshot(request)
        .await
        .expect("a response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    (status, String::from_utf8(body.to_vec()).expect("text"))
}

#[tokio::test]
async fn a_live_session_is_answered_with_its_account_and_its_roles() {
    let (status, body) = served(TwoOrgs, asking(Some(&format!("ct_session={TOKEN}")), true)).await;

    assert_eq!(status, StatusCode::OK);
    let standing: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(
        standing["account"],
        serde_json::json!(Uuid::from_u128(ACCOUNT).to_string())
    );
    assert_eq!(
        standing["active_org"],
        serde_json::json!(Uuid::from_u128(ACTIVE).to_string())
    );
    assert_eq!(
        standing["email"], "person@example.com",
        "a menu has an address to show rather than an identifier"
    );
    assert_eq!(
        standing["memberships"],
        serde_json::json!([
            {
                "org": Uuid::from_u128(ACTIVE).to_string(),
                "name": "Acme",
                "role": "owner",
            },
            {
                "org": Uuid::from_u128(OTHER).to_string(),
                "name": "Globex",
                "role": "member",
            },
        ])
    );
}

#[tokio::test]
async fn a_request_without_a_session_cookie_is_refused() {
    let (status, _) = served(TwoOrgs, asking(None, true)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_cookie_the_store_does_not_know_is_refused() {
    let (status, _) = served(TwoOrgs, asking(Some("ct_session=ct_ss_nope"), true)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_request_that_no_script_sent_is_refused() {
    let (status, _) = served(TwoOrgs, asking(Some(&format!("ct_session={TOKEN}")), false)).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the standing must not be readable by a plain cross site navigation"
    );
}

#[tokio::test]
async fn an_unreachable_control_plane_is_not_reported_as_a_missing_session() {
    let (status, _) = served(
        NoControlPlane,
        asking(Some(&format!("ct_session={TOKEN}")), true),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

fn an_email() -> comettrace_core::Email {
    comettrace_core::Email::try_from("person@example.com").expect("an address")
}
