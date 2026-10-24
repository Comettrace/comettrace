use comettrace_core::{Clock, IngestKey, SecretHash, TenantDirectory, TenantId, UnixSeconds};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const NANOS_PER_SECOND: i64 = 1_000_000_000;
const SIGHTING_GAP_SECONDS: u64 = 300;

#[derive(Debug, Clone)]
pub struct PostgresDirectory<C> {
    pool: PgPool,
    clock: C,
}

impl<C: Clock> PostgresDirectory<C> {
    pub fn new(pool: PgPool, clock: C) -> Self {
        Self { pool, clock }
    }

    async fn note_sighting(&self, key: Uuid, last_seen: Option<i64>) {
        let now = self.clock.now();
        let stale = last_seen.is_none_or(|seen| {
            nanos_to_seconds(seen)
                .saturating_add_secs(SIGHTING_GAP_SECONDS)
                .get()
                <= now.get()
        });
        if !stale {
            return;
        }
        let done = sqlx::query("UPDATE api_keys SET last_used_at = $2 WHERE id = $1")
            .bind(key)
            .bind(to_nanos(now))
            .execute(&self.pool)
            .await;
        if let Err(error) = done {
            tracing::warn!(%error, "a key sighting was not recorded");
        }
    }
}

impl<C: Clock + Send + Sync> TenantDirectory for PostgresDirectory<C> {
    async fn resolve(&self, key: &IngestKey) -> Result<TenantId, comettrace_core::UnknownKey> {
        let Some((prefix, secret)) = key.split() else {
            return Err(comettrace_core::UnknownKey);
        };

        let found = sqlx::query(
            "SELECT id AS id, org_id AS org_id, secret_digest AS secret_digest, \
             last_used_at AS last_used_at \
             FROM api_keys WHERE prefix = $1 AND revoked_at IS NULL",
        )
        .bind(prefix)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, "the control plane refused a key lookup");
            comettrace_core::UnknownKey
        })?;

        let offered = SecretHash::of(secret);
        let Some(row) = found else {
            return Err(comettrace_core::UnknownKey);
        };
        let Some(stored) = row.get::<Option<Vec<u8>>, _>("secret_digest") else {
            return Err(comettrace_core::UnknownKey);
        };
        let Ok(stored) = <[u8; 32]>::try_from(stored.as_slice()) else {
            return Err(comettrace_core::UnknownKey);
        };
        if SecretHash::from_bytes(stored) != offered {
            return Err(comettrace_core::UnknownKey);
        }

        self.note_sighting(row.get("id"), row.get("last_used_at"))
            .await;
        Ok(TenantId::new(row.get("org_id")))
    }
}

fn to_nanos(instant: UnixSeconds) -> i64 {
    i64::try_from(instant.get())
        .unwrap_or(i64::MAX / NANOS_PER_SECOND)
        .saturating_mul(NANOS_PER_SECOND)
}

fn nanos_to_seconds(nanos: i64) -> UnixSeconds {
    UnixSeconds::new(u64::try_from(nanos / NANOS_PER_SECOND).unwrap_or_default())
}
