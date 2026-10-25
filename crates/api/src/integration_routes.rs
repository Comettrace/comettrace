use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use comettrace_core::{Memberships, SessionStore, Whoami, WhoamiError};
use serde::Serialize;

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
