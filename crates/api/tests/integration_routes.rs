use std::sync::Arc;

use axum::body::Body;
use axum::http::header::COOKIE;
use axum::http::{Request, StatusCode};
use comettrace_api::{Repository, Site, Wired, integrations_router};
use comettrace_core::{
    AccountId, Membership, Memberships, OrgRole, Session, SessionError, SessionStore, SessionToken,
    TenantId, UnixSeconds, Whoami, WhoamiError,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "ct_ss_0123456789";

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

struct AMember;

impl Memberships for AMember {
    async fn of(&self, _account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
        Ok(vec![Membership {
            org: TenantId::new(Uuid::from_u128(1)),
            name: "Acme".into(),
            role: OrgRole::Member,
        }])
    }
}

fn everything() -> Wired {
    Wired {
        linear: true,
        github: Some(Repository {
            repository: "acme/storefront".to_owned(),
        }),
        jira: Some(Site {
            base_url: "https://acme.atlassian.net".to_owned(),
            project_key: "OPS".to_owned(),
        }),
        mail: true,
    }
}

fn asking(session: Option<&str>, scripted: bool) -> Request<Body> {
    let mut request = Request::builder().uri("/api/integrations");
    if let Some(token) = session {
        request = request.header(COOKIE, format!("ct_session={token}"));
    }
    if scripted {
        request = request.header("x-requested-with", "fetch");
    }
    request.body(Body::empty()).expect("a request")
}

async fn served(wired: Wired, request: Request<Body>) -> (StatusCode, String) {
    let whoami = Arc::new(Whoami::new(OneSession, AMember));
    let response = integrations_router(whoami, wired)
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
async fn a_signed_in_person_reads_what_the_deployment_is_wired_up_to() {
    let (status, body) = served(everything(), asking(Some(TOKEN), true)).await;

    assert_eq!(status, StatusCode::OK);
    let wired = read(&body);
    assert_eq!(wired["linear"], true);
    assert_eq!(wired["github"]["repository"], "acme/storefront");
    assert_eq!(wired["jira"]["base_url"], "https://acme.atlassian.net");
    assert_eq!(wired["jira"]["project_key"], "OPS");
    assert_eq!(wired["mail"], true);
}

/* The one thing this route must never carry. A token is read out of the configuration where it
was set and goes nowhere near a response; all anybody is told is that it is there. */
#[tokio::test]
async fn no_token_reaches_the_answer() {
    let (_, body) = served(everything(), asking(Some(TOKEN), true)).await;

    assert!(!body.contains("token"), "{body}");
    for secret in ["lin_api_", "ghp_", "ATATT"] {
        assert!(!body.contains(secret), "{body}");
    }
}

#[tokio::test]
async fn a_deployment_wired_to_nothing_says_so_rather_than_answering_nothing() {
    let (status, body) = served(Wired::default(), asking(Some(TOKEN), true)).await;

    assert_eq!(status, StatusCode::OK);
    let wired = read(&body);
    assert_eq!(wired["linear"], false);
    assert!(wired["github"].is_null());
    assert!(wired["jira"].is_null());
    assert_eq!(wired["mail"], false);
}

/* Unlike `/auth/methods`, which has to answer before anybody has signed in. What a deployment is
plugged into is nobody's business until they are in. */
#[tokio::test]
async fn a_request_carrying_no_session_is_told_to_sign_in() {
    let (status, _) = served(everything(), asking(None, true)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_session_nothing_carries_reads_nothing() {
    let (status, _) = served(everything(), asking(Some("ct_ss_nope"), true)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_request_from_another_origin_never_reaches_the_integrations() {
    let (status, _) = served(everything(), asking(Some(TOKEN), false)).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}
