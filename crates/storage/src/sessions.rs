use comettrace_core::{
    AccountId, Clock, SESSION_PREFIX, Session, SessionError, SessionLifetime, SessionStore,
    SessionToken, TenantId, UnixSeconds,
};
use rand::RngCore;
use sqlx::{PgPool, Row};

const TOKEN_BYTES: usize = 32;
const NANOS_PER_SECOND: i64 = 1_000_000_000;

#[derive(Debug, Clone)]
pub struct PostgresSessions<C> {
    pool: PgPool,
    clock: C,
    lifetime: SessionLifetime,
}

impl<C: Clock> PostgresSessions<C> {
    pub fn new(pool: PgPool, clock: C, lifetime: SessionLifetime) -> Self {
        Self {
            pool,
            clock,
            lifetime,
        }
    }

    pub async fn prune(&self) -> Result<u64, SessionError> {
        let now = seconds_to_nanos(self.clock.now());
        sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
            .bind(now)
            .execute(&self.pool)
            .await
            .map(|done| done.rows_affected())
            .map_err(|_| SessionError::StoreUnreachable)
    }
}

impl<C: Clock + Send + Sync> SessionStore for PostgresSessions<C> {
    async fn issue(
        &self,
        account: AccountId,
        org: Option<TenantId>,
    ) -> Result<SessionToken, SessionError> {
        let token = mint();
        let now = self.clock.now();
        let expires = self.lifetime.expiry_from(now, now);

        sqlx::query(
            "INSERT INTO sessions \
             (hash, account_id, org_id, created_at, last_seen_at, expires_at) \
             VALUES ($1, $2, $3, $4, $4, $5)",
        )
        .bind(token.hash().as_bytes().as_slice())
        .bind(account.as_uuid())
        .bind(org.map(TenantId::as_uuid))
        .bind(seconds_to_nanos(now))
        .bind(seconds_to_nanos(expires))
        .execute(&self.pool)
        .await
        .map_err(|_| SessionError::StoreUnreachable)?;

        Ok(token)
    }

    async fn resolve(&self, token: &SessionToken) -> Result<Session, SessionError> {
        let now = self.clock.now();
        let row = sqlx::query(
            "SELECT account_id AS account_id, org_id AS org_id, created_at AS created_at \
             FROM sessions \
             WHERE hash = $1 AND revoked_at IS NULL AND expires_at > $2",
        )
        .bind(token.hash().as_bytes().as_slice())
        .bind(seconds_to_nanos(now))
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| SessionError::StoreUnreachable)?
        .ok_or(SessionError::Unknown)?;

        let created = nanos_to_seconds(row.get("created_at"));
        let expires = self.lifetime.expiry_from(created, now);

        sqlx::query("UPDATE sessions SET last_seen_at = $2, expires_at = $3 WHERE hash = $1")
            .bind(token.hash().as_bytes().as_slice())
            .bind(seconds_to_nanos(now))
            .bind(seconds_to_nanos(expires))
            .execute(&self.pool)
            .await
            .map_err(|_| SessionError::StoreUnreachable)?;

        Ok(Session {
            account: AccountId::new(row.get("account_id")),
            org: row
                .get::<Option<uuid::Uuid>, _>("org_id")
                .map(TenantId::new),
            expires_at: expires,
        })
    }

    async fn revoke(&self, token: &SessionToken) -> Result<(), SessionError> {
        sqlx::query("UPDATE sessions SET revoked_at = $2 WHERE hash = $1 AND revoked_at IS NULL")
            .bind(token.hash().as_bytes().as_slice())
            .bind(seconds_to_nanos(self.clock.now()))
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|_| SessionError::StoreUnreachable)
    }
}

fn mint() -> SessionToken {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    SessionToken::from_plaintext(&format!("{SESSION_PREFIX}{}", hex::encode(bytes)))
}

fn seconds_to_nanos(instant: UnixSeconds) -> i64 {
    i64::try_from(instant.get())
        .unwrap_or(i64::MAX / NANOS_PER_SECOND)
        .saturating_mul(NANOS_PER_SECOND)
}

fn nanos_to_seconds(nanos: i64) -> UnixSeconds {
    UnixSeconds::new(u64::try_from(nanos / NANOS_PER_SECOND).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use comettrace_core::SESSION_PREFIX;

    use super::mint;

    #[test]
    fn a_minted_token_carries_the_prefix_and_enough_randomness() {
        let token = mint();

        assert!(token.looks_like_one());
        assert_eq!(token.exposed().len(), SESSION_PREFIX.len() + 64);
    }

    #[test]
    fn two_minted_tokens_differ() {
        assert_ne!(mint().exposed(), mint().exposed());
    }
}
