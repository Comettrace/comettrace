use core::fmt;

use comettrace_core::{Clock, DecompressedBytes, TenantId, UsageMeter, UsageReader};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use sqlx::{PgPool, Row};

use crate::month::BillingMonth;

const KEY_PREFIX: &str = "usage";
const KEY_TTL_SECONDS: i64 = 70 * 24 * 60 * 60;

#[derive(Debug, thiserror::Error)]
pub enum CounterError {
    #[error("the usage counter store refused the command")]
    Redis(#[from] redis::RedisError),
    #[error("the control plane refused the usage query")]
    ControlPlane(#[from] sqlx::Error),
}

pub struct RedisMeter<C> {
    connection: ConnectionManager,
    clock: C,
}

impl<C> fmt::Debug for RedisMeter<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("RedisMeter").finish_non_exhaustive()
    }
}

impl<C: Clock> RedisMeter<C> {
    pub async fn connect(url: &str, clock: C) -> Result<Self, CounterError> {
        let client = redis::Client::open(url)?;
        let connection = ConnectionManager::new(client).await?;
        Ok(Self { connection, clock })
    }

    fn key(&self, tenant: TenantId) -> String {
        let month = BillingMonth::containing(self.clock.now()).key();
        format!("{KEY_PREFIX}:{tenant}:{month}")
    }

    pub async fn seed(
        &self,
        tenant: TenantId,
        total: DecompressedBytes,
    ) -> Result<bool, CounterError> {
        let mut connection = self.connection.clone();
        let key = self.key(tenant);
        let claimed: bool = connection.set_nx(&key, total.get()).await?;
        if claimed {
            let _: bool = connection.expire(&key, KEY_TTL_SECONDS).await?;
        }
        Ok(claimed)
    }

    pub async fn rehydrate(&self, pool: &PgPool) -> Result<usize, CounterError> {
        let since = BillingMonth::containing(self.clock.now()).start_nanos();
        let rows = sqlx::query(
            "SELECT org_id AS org_id, sum(ingested_bytes)::bigint AS total \
             FROM usage_hourly WHERE hour_start >= $1 GROUP BY org_id",
        )
        .bind(since)
        .fetch_all(pool)
        .await?;

        let mut seeded = 0;
        for row in rows {
            let tenant = TenantId::new(row.get("org_id"));
            let total = u64::try_from(row.get::<i64, _>("total")).unwrap_or_default();
            if self.seed(tenant, DecompressedBytes::new(total)).await? {
                seeded += 1;
            }
        }
        Ok(seeded)
    }
}

impl<C: Clock + Send + Sync> UsageMeter for RedisMeter<C> {
    async fn record_ingest(&self, tenant: TenantId, decompressed: DecompressedBytes) {
        let mut connection = self.connection.clone();
        let key = self.key(tenant);
        match connection.incr::<_, _, u64>(&key, decompressed.get()).await {
            Ok(total) if total == decompressed.get() => {
                if let Err(error) = connection.expire::<_, bool>(&key, KEY_TTL_SECONDS).await {
                    tracing::error!(%error, %tenant, "the usage counter was left without an expiry");
                }
            }
            Ok(_) => {}
            Err(error) => {
                tracing::error!(%error, %tenant, "the usage counter was not incremented");
            }
        }
    }
}

impl<C: Clock + Send + Sync> UsageReader for RedisMeter<C> {
    async fn used(&self, tenant: TenantId) -> DecompressedBytes {
        let mut connection = self.connection.clone();
        match connection.get::<_, Option<u64>>(self.key(tenant)).await {
            Ok(total) => DecompressedBytes::new(total.unwrap_or_default()),
            Err(error) => {
                tracing::error!(
                    %error,
                    %tenant,
                    "the usage counter could not be read, so the payload is admitted"
                );
                DecompressedBytes::default()
            }
        }
    }
}
