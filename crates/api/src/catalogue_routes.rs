use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::get;
use comettrace_core::{Catalogue, Memberships, Projects, SessionStore, TenantId};
use serde::Serialize;
use uuid::Uuid;

use crate::explore_routes::{LookRefused, carried};

#[derive(Debug, Serialize)]
pub struct Held {
    projects: Vec<Kept>,
}

#[derive(Debug, Serialize)]
struct Kept {
    id: Uuid,
    name: String,
}

pub fn catalogue_router<S, G, P>(catalogue: Arc<Catalogue<S, G, P>>) -> Router
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    P: Projects + Send + Sync + 'static,
{
    Router::new()
        .route("/api/orgs/{org}/projects", get(projects::<S, G, P>))
        .with_state(catalogue)
}

async fn projects<S, G, P>(
    State(catalogue): State<Arc<Catalogue<S, G, P>>>,
    Path(org): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Held>, LookRefused>
where
    S: SessionStore + Send + Sync + 'static,
    G: Memberships + Send + Sync + 'static,
    P: Projects + Send + Sync + 'static,
{
    let token = carried(&headers)?;
    let held = catalogue
        .projects(&token, TenantId::new(org))
        .await?
        .into_iter()
        .map(|project| Kept {
            id: project.id.as_uuid(),
            name: project.name.into_string(),
        })
        .collect();
    Ok(Json(Held { projects: held }))
}
