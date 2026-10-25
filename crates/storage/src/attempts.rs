use std::time::Duration;

use comettrace_core::{
    AccountId, Attempt, Attempts, Clock, FlowError, Provider, TenantId, UnixSeconds,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const NANOS_PER_SECOND: i64 = 1_000_000_000;

/* Ten minutes, the same window an oidc sign-in attempt gets. It is how long somebody has to
read a provider's consent screen and press the button, which is the same problem either way. */
const DEFAULT_WINDOW: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct PostgresAttempts<C> {
    pool: PgPool,
    clock: C,
    window: Duration,
}

impl<C: Clock> PostgresAttempts<C> {
    pub fn new(pool: PgPool, clock: C) -> Self {
        Self {
            pool,
            clock,
            window: DEFAULT_WINDOW,
        }
    }

    #[must_use]
    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }

    /// Abandoned attempts, which is most of them: a consent screen is easy to close.
    pub async fn prune(&self) -> Result<u64, FlowError> {
        sqlx::query("DELETE FROM integration_attempts WHERE expires_at <= $1")
            .bind(to_nanos(self.clock.now()))
            .execute(&self.pool)
            .await
            .map(|done| done.rows_affected())
            .map_err(|_| FlowError::StoreUnreachable)
    }
}

impl<C: Clock + Send + Sync> Attempts for PostgresAttempts<C> {
    async fn begin(&self, attempt: &Attempt) -> Result<(), FlowError> {
        let now = self.clock.now();
        let expires = now.saturating_add_secs(self.window.as_secs());

        sqlx::query(
            "INSERT INTO integration_attempts \
             (state, org_id, account_id, kind, created_at, expires_at) \
             VALUES ($1, $2, $3, $4::integration_kind, $5, $6)",
        )
        .bind(&attempt.state)
        .bind(attempt.org.as_uuid())
        .bind(attempt.by.as_uuid())
        .bind(attempt.provider.as_str())
        .bind(to_nanos(now))
        .bind(to_nanos(expires))
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|_| FlowError::StoreUnreachable)
    }

    /* Deleted on the way out, in the statement that reads it, so a state that comes back twice
    works once however many requests arrive at once. An expired row is left for `prune` and reads
    as unknown, which is the same answer as a state nobody ever minted — a caller cannot tell a
    stale attempt from an invented one, and does not need to. */
    async fn take(&self, state: &str) -> Result<Attempt, FlowError> {
        let row = sqlx::query(
            "DELETE FROM integration_attempts WHERE state = $1 AND expires_at > $2 \
             RETURNING org_id AS org_id, account_id AS account_id, kind::text AS kind",
        )
        .bind(state)
        .bind(to_nanos(self.clock.now()))
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| FlowError::StoreUnreachable)?
        .ok_or(FlowError::Unknown)?;

        let spelling: String = row.get("kind");
        let provider = Provider::read(&spelling).ok_or_else(|| {
            tracing::error!(kind = %spelling, "the control plane holds an unknown provider");
            FlowError::StoreUnreachable
        })?;

        Ok(Attempt {
            state: state.to_owned(),
            org: TenantId::new(row.get::<Uuid, _>("org_id")),
            by: AccountId::new(row.get::<Uuid, _>("account_id")),
            provider,
        })
    }
}

fn to_nanos(instant: UnixSeconds) -> i64 {
    i64::try_from(instant.get())
        .unwrap_or(i64::MAX / NANOS_PER_SECOND)
        .saturating_mul(NANOS_PER_SECOND)
}
