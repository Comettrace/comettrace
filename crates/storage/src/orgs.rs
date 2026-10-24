use comettrace_core::{OrgName, Organisation, Orgs, ReadError, TenantId};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PostgresOrgs {
    pool: PgPool,
}

impl PostgresOrgs {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Orgs for PostgresOrgs {
    async fn only(&self, org: TenantId) -> Result<Option<Organisation>, ReadError> {
        let found = sqlx::query(
            "SELECT o.name AS name, p.name AS plan, o.created_at AS created_at \
             FROM orgs o JOIN plans p ON p.id = o.plan_id \
             WHERE o.id = $1",
        )
        .bind(org.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, "the control plane refused to read an organisation");
            ReadError::StoreUnreachable
        })?;

        Ok(found.map(|row| Organisation {
            id: org,
            name: row.get::<String, _>("name").into(),
            plan: row.get::<String, _>("plan").into(),
            created_at_unix_nanos: u64::try_from(row.get::<i64, _>("created_at"))
                .unwrap_or_default(),
        }))
    }

    async fn rename(&self, org: TenantId, name: &OrgName) -> Result<(), ReadError> {
        sqlx::query("UPDATE orgs SET name = $2 WHERE id = $1")
            .bind(org.as_uuid())
            .bind(name.as_str())
            .execute(&self.pool)
            .await
            .map_err(|error| {
                tracing::error!(%error, "the control plane refused to rename an organisation");
                ReadError::StoreUnreachable
            })?;
        Ok(())
    }
}
