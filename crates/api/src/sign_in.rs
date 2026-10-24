use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::header::{HeaderName, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use comettrace_core::{
    AccountDirectory, AuthError, AuthProvider, Email, Password, SessionError, SessionStore, SignIn,
    SignInError, SignInProof,
};
use serde::Deserialize;

use crate::session_cookie::{self, SessionCookie};

const REQUESTED_WITH: HeaderName = HeaderName::from_static("x-requested-with");

#[derive(Debug, Deserialize)]
pub struct Credentials {
    email: String,
    password: String,
}

#[derive(Debug, Clone, Copy)]
struct Shape {
    cookie: SessionCookie,
}

pub fn auth_router<L, F, A, S>(sign_in: Arc<SignIn<L, F, A, S>>, cookie: SessionCookie) -> Router
where
    L: AuthProvider + Send + Sync + 'static,
    F: AuthProvider + Send + Sync + 'static,
    A: AccountDirectory + Send + Sync + 'static,
    S: SessionStore + Send + Sync + 'static,
{
    Router::new()
        .route("/auth/sign-in", post(begin::<L, F, A, S>))
        .route("/auth/sign-out", post(end::<L, F, A, S>))
        .layer(axum::Extension(Shape { cookie }))
        .with_state(sign_in)
}

async fn begin<L, F, A, S>(
    State(sign_in): State<Arc<SignIn<L, F, A, S>>>,
    axum::Extension(shape): axum::Extension<Shape>,
    headers: HeaderMap,
    Json(credentials): Json<Credentials>,
) -> Result<Response, SignInRefused>
where
    L: AuthProvider + Send + Sync + 'static,
    F: AuthProvider + Send + Sync + 'static,
    A: AccountDirectory + Send + Sync + 'static,
    S: SessionStore + Send + Sync + 'static,
{
    requested_by_a_script(&headers)?;
    let email =
        Email::try_from(credentials.email.as_str()).map_err(|_| SignInRefused::Credentials)?;
    let proof = SignInProof::Password {
        email,
        password: Password::new(&credentials.password),
    };

    let token = sign_in.begin(proof).await?;
    let Some(cookie) = session_cookie::opened(&token, shape.cookie) else {
        return Err(SignInRefused::Unavailable);
    };
    Ok(([(SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response())
}

async fn end<L, F, A, S>(
    State(sign_in): State<Arc<SignIn<L, F, A, S>>>,
    axum::Extension(shape): axum::Extension<Shape>,
    headers: HeaderMap,
) -> Result<Response, SignInRefused>
where
    L: AuthProvider + Send + Sync + 'static,
    F: AuthProvider + Send + Sync + 'static,
    A: AccountDirectory + Send + Sync + 'static,
    S: SessionStore + Send + Sync + 'static,
{
    requested_by_a_script(&headers)?;
    if let Some(token) = session_cookie::carried(&headers) {
        sign_in.end(&token).await?;
    }
    let Some(cookie) = session_cookie::cleared(shape.cookie) else {
        return Err(SignInRefused::Unavailable);
    };
    Ok(([(SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response())
}

fn requested_by_a_script(headers: &HeaderMap) -> Result<(), SignInRefused> {
    if headers.contains_key(&REQUESTED_WITH) {
        Ok(())
    } else {
        Err(SignInRefused::NotFromOurOrigin)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SignInRefused {
    #[error("the request carries no x-requested-with header")]
    NotFromOurOrigin,
    #[error("the credentials match no account")]
    Credentials,
    #[error("this way of signing in is not enabled")]
    NotEnabled,
    #[error("the sign in could not be completed")]
    Unavailable,
}

impl From<SignInError> for SignInRefused {
    fn from(error: SignInError) -> Self {
        match error {
            SignInError::Auth(AuthError::Rejected)
            | SignInError::Session(SessionError::Unknown) => Self::Credentials,
            SignInError::Auth(AuthError::SourceDisabled) => Self::NotEnabled,
            SignInError::Auth(AuthError::ProviderUnreachable)
            | SignInError::Session(SessionError::StoreUnreachable) => Self::Unavailable,
        }
    }
}
