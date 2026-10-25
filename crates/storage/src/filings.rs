use std::collections::BTreeMap;

use comettrace_core::{
    ConnectionError, Destination, Filings, ProjectId, Provider, Routing, TenantId,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/* Where issues go. The default lives on the connection row, because a destination means nothing
without the workspace it is in and that row is what names the workspace; the projects that differ
live in a table of their own. */
#[derive(Debug, Clone)]
pub struct PostgresFilings {
    pool: PgPool,
}

impl PostgresFilings {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Filings for PostgresFilings {
    /* Two reads rather than an outer join, for the same reason the teams read three: the rows
    would come back multiplied and then have to be folded anyway, and a provider with a default
    and no overrides would be indistinguishable from one with neither until the fold.

    The first order by is qualified for the reason the connections read is: a bare `kind` would
    sort the text cast rather than the enum. */
    async fn of(&self, org: TenantId) -> Result<Vec<Routing>, ConnectionError> {
        let connections = sqlx::query(
            "SELECT kind::text AS kind, destination_id AS destination_id, \
             destination_name AS destination_name \
             FROM integrations WHERE org_id = $1 ORDER BY integrations.kind",
        )
        .bind(org.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| unreachable(&error, "read where issues go"))?;

        let aimed = sqlx::query(
            "SELECT kind::text AS kind, project_id AS project_id, \
             destination_id AS destination_id, destination_name AS destination_name \
             FROM issue_destinations WHERE org_id = $1 ORDER BY destination_name, project_id",
        )
        .bind(org.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| unreachable(&error, "read which projects differ"))?;

        let mut overrides: BTreeMap<Provider, Vec<(ProjectId, Destination)>> = BTreeMap::new();
        for row in &aimed {
            let provider = named(row)?;
            overrides.entry(provider).or_default().push((
                ProjectId::new(row.get::<Uuid, _>("project_id")),
                destination(row).ok_or(ConnectionError::StoreUnreachable)?,
            ));
        }

        connections
            .iter()
            .map(|row| {
                let provider = named(row)?;
                Ok(Routing {
                    provider,
                    default: destination(row),
                    projects: overrides.remove(&provider).unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn aim(
        &self,
        org: TenantId,
        provider: Provider,
        project: Option<ProjectId>,
        destination: Option<&Destination>,
    ) -> Result<(), ConnectionError> {
        let Some(project) = project else {
            /* Scoped by the organisation as well as the kind, so a connection of another
            organisation is a row this statement does not reach. */
            sqlx::query(
                "UPDATE integrations SET destination_id = $3, destination_name = $4 \
                 WHERE org_id = $1 AND kind = $2::integration_kind",
            )
            .bind(org.as_uuid())
            .bind(provider.as_str())
            .bind(destination.map(|aimed| &*aimed.id))
            .bind(destination.map(|aimed| &*aimed.name))
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| unreachable(&error, "aim a connection"))?;
            return Ok(());
        };

        let Some(aimed) = destination else {
            /* Clearing a project is deleting its row: a project with nothing of its own follows
            the default, and a row saying so would be a second way to spell the same thing. */
            sqlx::query(
                "DELETE FROM issue_destinations \
                 WHERE org_id = $1 AND project_id = $2 AND kind = $3::integration_kind",
            )
            .bind(org.as_uuid())
            .bind(project.as_uuid())
            .bind(provider.as_str())
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| unreachable(&error, "let a project follow the default"))?;
            return Ok(());
        };

        /* The two foreign keys do the checking: one refuses a project of another organisation,
        the other refuses a provider this organisation is not connected to. Neither needs asking
        about first, and a refusal from either is a request that was wrong rather than a store
        that is broken — which is why they are told apart below. */
        sqlx::query(
            "INSERT INTO issue_destinations \
             (org_id, project_id, kind, destination_id, destination_name) \
             VALUES ($1, $2, $3::integration_kind, $4, $5) \
             ON CONFLICT (project_id, kind) DO UPDATE SET \
             destination_id = $4, destination_name = $5",
        )
        .bind(org.as_uuid())
        .bind(project.as_uuid())
        .bind(provider.as_str())
        .bind(&*aimed.id)
        .bind(&*aimed.name)
        .execute(&self.pool)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(refusal) if refusal.is_foreign_key_violation() => {
                ConnectionError::NotConnected
            }
            _ => unreachable(&error, "aim a project"),
        })?;
        Ok(())
    }
}

fn named(row: &sqlx::postgres::PgRow) -> Result<Provider, ConnectionError> {
    let spelling: String = row.get("kind");
    Provider::read(&spelling).ok_or_else(|| {
        tracing::error!(kind = %spelling, "the control plane holds an unknown provider");
        ConnectionError::StoreUnreachable
    })
}

/* Both columns or neither, which the check constraint on the table also says. */
fn destination(row: &sqlx::postgres::PgRow) -> Option<Destination> {
    let id: Option<String> = row.get("destination_id");
    let name: Option<String> = row.get("destination_name");
    Some(Destination {
        id: id?.into(),
        name: name?.into(),
    })
}

fn unreachable(error: &sqlx::Error, what: &str) -> ConnectionError {
    tracing::error!(%error, "the control plane refused to {what}");
    ConnectionError::StoreUnreachable
}
