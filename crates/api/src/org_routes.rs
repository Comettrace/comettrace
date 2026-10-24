use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use comettrace_core::{
    BadName, Memberships, OrgName, OrgSettings, Organisation, Orgs, SessionStore, SettingError,
    TenantId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::explore_routes::{NotCarried, carried};

const NANOS_PER_MILLI: u64 = 1_000_000;

/* Milliseconds, like every other instant this api carries: a nanosecond count passes 2^53 in
1970 and javascript would round it. The version is a fact about the binary rather than about the
organisation, and it rides along because the screen shows both in one table. */
#[derive(Debug, Serialize)]
pub struct Card {
    id: Uuid,
    name: String,
    plan: String,
    created_at_ms: u64,
    version: &'static str,
}

impl From<Organisation> for Card {
    fn from(org: Organisation) -> Self {
        Self {
            id: org.id.as_uuid(),
            name: org.name.into_string(),
            plan: org.plan.into_string(),
            created_at_ms: org.created_at_unix_nanos / NANOS_PER_MILLI,
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Renaming {
    name: String,
}

pub fn org_router<S, G, O>(settings: Arc<OrgSettings<S, G, O>>) -> Router
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/orgs/{org}",
            get(card::<S, G, O>).patch(rename::<S, G, O>),
        )
        .with_state(settings)
}

async fn card<S, G, O>(
    State(settings): State<Arc<OrgSettings<S, G, O>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Card>, OrgRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let card = settings.read(&token, TenantId::new(org)).await?;
    Ok(Json(card.into()))
}

async fn rename<S, G, O>(
    State(settings): State<Arc<OrgSettings<S, G, O>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
    Json(asked): Json<Renaming>,
) -> Result<Json<Card>, OrgRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let name = OrgName::try_from(asked.name.as_str())?;
    let card = settings.rename(&token, TenantId::new(org), &name).await?;
    tracing::info!(org = %org, "an organisation was renamed");
    Ok(Json(card.into()))
}

#[derive(Debug, thiserror::Error)]
pub enum OrgRefused {
    #[error(transparent)]
    Reaching(#[from] NotCarried),
    #[error(transparent)]
    Name(#[from] BadName),
    #[error("no such organisation")]
    NoSuchOrg,
    #[error("changing an organisation is an administrator's to do")]
    NotAnAdministrator,
    #[error("the organisation could not be read")]
    Unavailable,
}

/* A session that holds no membership of the organisation is told there is none, so the ids of
other organisations stay unguessable. A member who is simply not an administrator is told the
truth instead: it already knows the organisation exists, and a 404 would only confuse it. */
impl From<SettingError> for OrgRefused {
    fn from(error: SettingError) -> Self {
        match error {
            SettingError::NoSession => Self::Reaching(NotCarried::NoSession),
            SettingError::NotYours | SettingError::Gone => Self::NoSuchOrg,
            SettingError::NotYoursToChange => Self::NotAnAdministrator,
            SettingError::StoreUnreachable => Self::Unavailable,
        }
    }
}

impl IntoResponse for OrgRefused {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Reaching(refused) => return refused.into_response(),
            Self::Name(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NoSuchOrg => StatusCode::NOT_FOUND,
            Self::NotAnAdministrator => StatusCode::FORBIDDEN,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "an organisation could not be served");
        }
        (status, self.to_string()).into_response()
    }
}
