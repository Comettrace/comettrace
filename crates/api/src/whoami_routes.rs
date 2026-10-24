use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::header::HeaderName;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use comettrace_core::{Memberships, SessionStore, TenantId, Who, Whoami, WhoamiError};
use serde::Serialize;
use uuid::Uuid;

use crate::session_cookie;

const REQUESTED_WITH: HeaderName = HeaderName::from_static("x-requested-with");

#[derive(Debug, Serialize)]
pub struct Standing {
    account: Uuid,
    active_org: Option<Uuid>,
    memberships: Vec<Belonging>,
}

#[derive(Debug, Serialize)]
struct Belonging {
    org: Uuid,
    role: &'static str,
}

impl From<Who> for Standing {
    fn from(who: Who) -> Self {
        Self {
            account: who.account.as_uuid(),
            active_org: who.active_org.map(TenantId::as_uuid),
            memberships: who
                .memberships
                .into_iter()
                .map(|membership| Belonging {
                    org: membership.org.as_uuid(),
                    role: membership.role.as_str(),
                })
                .collect(),
        }
    }
}

pub fn me_router<S, G>(whoami: Arc<Whoami<S, G>>) -> Router
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
{
    Router::new()
        .route("/auth/me", get(standing::<S, G>))
        .with_state(whoami)
}

async fn standing<S, G>(
    State(whoami): State<Arc<Whoami<S, G>>>,
    headers: HeaderMap,
) -> Result<Json<Standing>, StandingRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
{
    if !headers.contains_key(&REQUESTED_WITH) {
        return Err(StandingRefused::NotFromOurOrigin);
    }
    let token = session_cookie::carried(&headers).ok_or(StandingRefused::NoSession)?;

    Ok(Json(whoami.of(&token).await?.into()))
}

#[derive(Debug, thiserror::Error)]
pub enum StandingRefused {
    #[error("the request carries no x-requested-with header")]
    NotFromOurOrigin,
    #[error("the request carries no live session")]
    NoSession,
    #[error("the standing could not be read")]
    Unavailable,
}

impl From<WhoamiError> for StandingRefused {
    fn from(error: WhoamiError) -> Self {
        match error {
            WhoamiError::NoSession => Self::NoSession,
            WhoamiError::StoreUnreachable => Self::Unavailable,
        }
    }
}

impl IntoResponse for StandingRefused {
    fn into_response(self) -> Response {
        let status = match self {
            Self::NotFromOurOrigin => StatusCode::FORBIDDEN,
            Self::NoSession => StatusCode::UNAUTHORIZED,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "a standing could not be served");
        }
        (status, self.to_string()).into_response()
    }
}
