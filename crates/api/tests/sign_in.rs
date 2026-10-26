use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, COOKIE, SET_COOKIE};
use axum::http::{Request, StatusCode};
use comettrace_api::{SessionCookie, auth_router};
use comettrace_core::{
    AccountDirectory, AccountId, AuthError, AuthProvider, AuthSources, Issuer, NoProvider,
    ProvisionPolicy, ResolvedIdentity, Session, SessionError, SessionStore, SessionToken, SignIn,
    SignInProof, Subject, TenantId, UnixSeconds,
};
use tower::ServiceExt;
use uuid::Uuid;

const GOOD: &str = "correct horse";
const TOKEN: &str = "ct_ss_0123456789";

struct OnePassword;

impl AuthProvider for OnePassword {
    async fn sign_in(&self, proof: SignInProof) -> Result<ResolvedIdentity, AuthError> {
        let SignInProof::Password { email, password } = proof else {
            return Err(AuthError::SourceDisabled);
        };
        if password.exposed() != GOOD {
            return Err(AuthError::Rejected);
        }
        Ok(ResolvedIdentity {
            issuer: Issuer::Local,
            subject: Subject::try_from("subject").expect("a subject"),
            email,
            email_verified: true,
        })
    }
}

struct OneAccount(AccountId);

impl AccountDirectory for OneAccount {
    async fn provision(
        &self,
        _identity: &ResolvedIdentity,
        _policy: ProvisionPolicy,
    ) -> Result<AccountId, AuthError> {
        Ok(self.0)
    }
}

#[derive(Default)]
struct MemorySessions {
    live: Mutex<Vec<String>>,
}

impl MemorySessions {
    fn holds(&self, token: &str) -> bool {
        self.live
            .lock()
            .expect("the session lock")
            .iter()
            .any(|held| held == token)
    }
}

impl SessionStore for MemorySessions {
    async fn issue(
        &self,
        account: AccountId,
        org: Option<TenantId>,
    ) -> Result<SessionToken, SessionError> {
        let _ = (account, org);
        self.live
            .lock()
            .expect("the session lock")
            .push(TOKEN.to_owned());
        Ok(SessionToken::from_plaintext(TOKEN))
    }

    async fn resolve(&self, token: &SessionToken) -> Result<Session, SessionError> {
        if self.holds(token.exposed()) {
            Ok(Session {
                account: AccountId::new(Uuid::nil()),
                email: an_email(),
                name: None,
                photo: None,
                org: None,
                expires_at: UnixSeconds::new(0),
            })
        } else {
            Err(SessionError::Unknown)
        }
    }

    async fn revoke(&self, token: &SessionToken) -> Result<(), SessionError> {
        self.live
            .lock()
            .expect("the session lock")
            .retain(|held| held != token.exposed());
        Ok(())
    }
}

type Service = SignIn<OnePassword, NoProvider, OneAccount, Arc<MemorySessions>>;

fn harness() -> (axum::Router, Arc<MemorySessions>) {
    let sessions = Arc::new(MemorySessions::default());
    let sign_in: Arc<Service> = Arc::new(SignIn::new(
        AuthSources::new(Some(OnePassword), None),
        OneAccount(AccountId::new(Uuid::from_u128(7))),
        Arc::clone(&sessions),
        ProvisionPolicy::default(),
    ));
    let router = auth_router(
        sign_in,
        SessionCookie {
            secure: true,
            max_age_seconds: 600,
        },
    );
    (router, sessions)
}

fn sign_in_request(password: &str, with_header: bool) -> Request<Body> {
    let body = format!(r#"{{"email":"person@example.com","password":"{password}"}}"#);
    let mut builder = Request::builder()
        .method("POST")
        .uri("/auth/sign-in")
        .header(CONTENT_TYPE, "application/json");
    if with_header {
        builder = builder.header("x-requested-with", "comettrace");
    }
    builder.body(Body::from(body)).expect("a request")
}

#[tokio::test]
async fn the_right_password_opens_a_session_cookie() {
    let (router, sessions) = harness();

    let response = router
        .oneshot(sign_in_request(GOOD, true))
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookie = response
        .headers()
        .get(SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("a cookie");
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("SameSite=Lax"), "{cookie}");
    assert!(cookie.contains("Secure"), "{cookie}");
    assert!(sessions.holds(TOKEN));
}

#[tokio::test]
async fn a_wrong_password_opens_nothing() {
    let (router, sessions) = harness();

    let response = router
        .oneshot(sign_in_request("wrong horse", true))
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().get(SET_COOKIE).is_none());
    assert!(!sessions.holds(TOKEN));
}

#[tokio::test]
async fn a_request_from_another_origin_is_refused_before_the_password_is_read() {
    let (router, sessions) = harness();

    let response = router
        .oneshot(sign_in_request(GOOD, false))
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!sessions.holds(TOKEN));
}

#[tokio::test]
async fn signing_out_revokes_the_session_and_clears_the_cookie() {
    let (router, sessions) = harness();
    router
        .clone()
        .oneshot(sign_in_request(GOOD, true))
        .await
        .expect("the sign in");
    assert!(sessions.holds(TOKEN));

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/sign-out")
                .header("x-requested-with", "comettrace")
                .header(COOKIE, format!("ct_session={TOKEN}"))
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookie = response
        .headers()
        .get(SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("a cookie");
    assert!(cookie.contains("Max-Age=0"), "{cookie}");
    assert!(!sessions.holds(TOKEN));
}

#[tokio::test]
async fn signing_out_without_a_cookie_still_clears_it() {
    let (router, _sessions) = harness();

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/sign-out")
                .header("x-requested-with", "comettrace")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(response.headers().get(SET_COOKIE).is_some());
}

fn an_email() -> comettrace_core::Email {
    comettrace_core::Email::try_from("person@example.com").expect("an address")
}
