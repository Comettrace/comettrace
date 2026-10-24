use comettrace_core::{Project, ProjectId, Projects, ReadError, TenantId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("the control plane refused the project query")]
    ControlPlane(#[from] sqlx::Error),
    #[error("no organisation carries the id {0}")]
    NoSuchOrg(Uuid),
    #[error("that organisation already holds a project called {0}")]
    Taken(String),
}

#[derive(Debug, Clone)]
pub struct PostgresProjects {
    pool: PgPool,
}

impl PostgresProjects {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, org: TenantId, name: &str) -> Result<ProjectId, ProjectError> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id, org_id, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(org.as_uuid())
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|error| match &error {
                sqlx::Error::Database(refusal) if refusal.is_unique_violation() => {
                    ProjectError::Taken(name.to_owned())
                }
                sqlx::Error::Database(refusal) if refusal.is_foreign_key_violation() => {
                    ProjectError::NoSuchOrg(org.as_uuid())
                }
                _ => ProjectError::ControlPlane(error),
            })?;
        Ok(ProjectId::new(id))
    }

    /* The only project of an organisation is the one a key belongs to when nobody named one, which
    is the single project case every self-hosted deployment is. More than one and the choice is
    the operator's to make. */
    pub async fn only(&self, org: TenantId) -> Result<Option<ProjectId>, ProjectError> {
        let found = sqlx::query(
            "SELECT id AS id FROM projects WHERE org_id = $1 ORDER BY created_at LIMIT 2",
        )
        .bind(org.as_uuid())
        .fetch_all(&self.pool)
        .await?;
        if found.len() == 1 {
            Ok(Some(ProjectId::new(found[0].get("id"))))
        } else {
            Ok(None)
        }
    }
}

impl Projects for PostgresProjects {
    async fn of(&self, tenant: TenantId) -> Result<Vec<Project>, ReadError> {
        let rows = sqlx::query(
            "SELECT id AS id, name AS name FROM projects WHERE org_id = $1 ORDER BY name, id",
        )
        .bind(tenant.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, "the control plane refused to read projects");
            ReadError::StoreUnreachable
        })?;

        Ok(rows
            .into_iter()
            .map(|row| Project {
                id: ProjectId::new(row.get("id")),
                org: tenant,
                name: row.get::<String, _>("name").into(),
            })
            .collect())
    }
}
