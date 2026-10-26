use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use comettrace_core::{
    BadName, BadPhoto, MAX_PHOTO_BYTES, Memberships, OrgName, OrgSettings, Organisation, Orgs,
    Photo, PhotoError, Photos, Portrait, SessionStore, SettingError, TenantId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::explore_routes::{NotCarried, carried};
use crate::session_cookie;

const NANOS_PER_MILLI: u64 = 1_000_000;

/* The url of a logo carries the token the store named it after, so it is a different url every
time an administrator replaces the mark. That is what lets it be cached for a year: no reader is
ever served the logo somebody deleted. Private, because a live session is what the route asks
for. */
const FOR_EVER: &str = "private, max-age=31536000, immutable";

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
    /* A url rather than a flag beside an id, so no screen has to know how one is spelt. Absent
    until an administrator keeps one, and the screens show the initial instead. */
    logo: Option<String>,
}

pub fn logo_url(org: Uuid, portrait: &Portrait) -> String {
    format!("/api/orgs/{org}/logo?v={}", portrait.token)
}

impl From<Organisation> for Card {
    fn from(org: Organisation) -> Self {
        Self {
            id: org.id.as_uuid(),
            name: org.name.into_string(),
            plan: org.plan.into_string(),
            created_at_ms: org.created_at_unix_nanos / NANOS_PER_MILLI,
            version: env!("CARGO_PKG_VERSION"),
            logo: org
                .logo
                .as_ref()
                .map(|portrait| logo_url(org.id.as_uuid(), portrait)),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Renaming {
    name: String,
}

pub fn org_router<S, G, O, P>(settings: Arc<OrgSettings<S, G, O, P>>) -> Router
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
    P: Photos + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/orgs/{org}",
            get(card::<S, G, O, P>).patch(rename::<S, G, O, P>),
        )
        .route(
            "/api/orgs/{org}/logo",
            get(served::<S, G, O, P>)
                .post(keep::<S, G, O, P>)
                .delete(forget::<S, G, O, P>)
                /* The ceiling the core type enforces, said again here so a body past it is
                refused as it arrives rather than after a megabyte of it is held. */
                .layer(DefaultBodyLimit::max(MAX_PHOTO_BYTES)),
        )
        .with_state(settings)
}

async fn card<S, G, O, P>(
    State(settings): State<Arc<OrgSettings<S, G, O, P>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Card>, OrgRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
    P: Photos + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let card = settings.read(&token, TenantId::new(org)).await?;
    Ok(Json(card.into()))
}

async fn rename<S, G, O, P>(
    State(settings): State<Arc<OrgSettings<S, G, O, P>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
    Json(asked): Json<Renaming>,
) -> Result<Json<Card>, OrgRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
    P: Photos + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let name = OrgName::try_from(asked.name.as_str())?;
    let card = settings.rename(&token, TenantId::new(org), &name).await?;
    tracing::info!(org = %org, "an organisation was renamed");
    Ok(Json(card.into()))
}

/* The bytes are the body, and what they are is read from the bytes themselves rather than from
the content type the sender announced: a logo is handed back to every reader's browser under our
own origin, so a file that is really a script must not get that far. */
async fn keep<S, G, O, P>(
    State(settings): State<Arc<OrgSettings<S, G, O, P>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Card>, OrgRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
    P: Photos + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let logo = Photo::examined(body.to_vec())?;
    let kind = logo.kind();

    let card = settings.put_logo(&token, TenantId::new(org), &logo).await?;
    tracing::info!(org = %org, %kind, "an organisation kept a logo");
    Ok(Json(card.into()))
}

async fn forget<S, G, O, P>(
    State(settings): State<Arc<OrgSettings<S, G, O, P>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Card>, OrgRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
    P: Photos + Send + Sync + 'static,
{
    let token = carried(&headers)?;

    let card = settings.remove_logo(&token, TenantId::new(org)).await?;
    tracing::info!(org = %org, "an organisation dropped its logo");
    Ok(Json(card.into()))
}

/* The one route here that cannot ask for the x-requested-with header: this is what an <img src>
fetches, and an image element sends no headers of its own. The session cookie is SameSite=Lax, so
a page on another site that embeds one of these urls has no cookie attached to it and is answered
401 -- the header would be guarding a door that is already shut. */
async fn served<S, G, O, P>(
    State(settings): State<Arc<OrgSettings<S, G, O, P>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, OrgRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    O: Orgs + Send + Sync + 'static,
    P: Photos + Send + Sync + 'static,
{
    let token = session_cookie::carried(&headers)
        .ok_or(OrgRefused::Reaching(NotCarried::NoSession))?;
    let (kind, bytes) = settings.logo(&token, TenantId::new(org)).await?;

    Ok((
        [(CONTENT_TYPE, kind.as_str()), (CACHE_CONTROL, FOR_EVER)],
        bytes,
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum OrgRefused {
    #[error(transparent)]
    Reaching(#[from] NotCarried),
    #[error(transparent)]
    Name(#[from] BadName),
    #[error(transparent)]
    Picture(#[from] BadPhoto),
    #[error("that organisation keeps no logo")]
    NoLogo,
    #[error(transparent)]
    NoStore(PhotoError),
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
            SettingError::NoLogo => Self::NoLogo,
            SettingError::Photos(refusal) => Self::NoStore(refusal),
            SettingError::StoreUnreachable => Self::Unavailable,
        }
    }
}

impl IntoResponse for OrgRefused {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Reaching(refused) => return refused.into_response(),
            Self::Name(_) | Self::Picture(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NoSuchOrg | Self::NoLogo => StatusCode::NOT_FOUND,
            Self::NotAnAdministrator => StatusCode::FORBIDDEN,
            /* A deployment that keeps no logos is a deployment that is missing a part, which is
            an operator's to fix rather than a reader's: the sentence says which part. */
            Self::NoStore(_) | Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "an organisation could not be served");
        }
        (status, self.to_string()).into_response()
    }
}
