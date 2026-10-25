use comettrace_core::{
    AccountId, Connecting, Connection, ConnectionError, Connections, Provider, ProviderToken,
    Sealed, TenantId, Vault,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/* The store holds the vault, so sealing is this layer's business and the port above speaks in
plain tokens. It is the same arrangement the roster has with the password hasher: the thing that
writes the row is the thing that knows how the row protects itself.

The vault is generic rather than concrete because a deployment that set no key still has this
store — it reads what is connected, it just cannot connect anything. `NoVault` is what it holds
then, and the refusal says so. */
#[derive(Debug, Clone)]
pub struct PostgresConnections<V> {
    pool: PgPool,
    vault: V,
}

impl<V> PostgresConnections<V> {
    pub fn new(pool: PgPool, vault: V) -> Self {
        Self { pool, vault }
    }
}

impl<V: Vault + Send + Sync> Connections for PostgresConnections<V> {
    async fn of(&self, org: TenantId) -> Result<Vec<Connection>, ConnectionError> {
        /* The sealed columns are deliberately not selected. Nothing on a connection card needs
        them, and a query that does not ask for a secret cannot spill one.

        The order by is qualified on purpose. A bare `kind` would bind to the output column, which
        is the text cast, and sort the providers alphabetically; naming the table reaches the enum
        column, which sorts them in the order the type declares — the order the screen reads. */
        let rows = sqlx::query(
            "SELECT kind::text AS kind, account_label AS account_label, \
             connected_by AS connected_by, created_at AS created_at \
             FROM integrations WHERE org_id = $1 ORDER BY integrations.kind",
        )
        .bind(org.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| unreachable(&error, "read the connections"))?;

        rows.into_iter()
            .map(|row| {
                let spelling: String = row.get("kind");
                let provider = Provider::read(&spelling).ok_or_else(|| {
                    tracing::error!(kind = %spelling, "the control plane holds an unknown provider");
                    ConnectionError::StoreUnreachable
                })?;
                Ok(Connection {
                    provider,
                    account_label: row.get::<String, _>("account_label").into(),
                    connected_at_unix_nanos: u64::try_from(row.get::<i64, _>("created_at"))
                        .unwrap_or_default(),
                    connected_by: row
                        .get::<Option<Uuid>, _>("connected_by")
                        .map(AccountId::new),
                })
            })
            .collect()
    }

    /* Sealed before the statement is built, so a deployment with no key is refused before it
    writes anything. Connecting again replaces what was there, because that is what reconnecting
    means — and it draws a fresh nonce, so the new row shares nothing with the old one. */
    async fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> Result<(), ConnectionError> {
        let sealed = self.vault.seal(connecting.credential)?;

        sqlx::query(
            "INSERT INTO integrations \
             (org_id, kind, nonce, secret, account_label, connected_by) \
             VALUES ($1, $2::integration_kind, $3, $4, $5, $6) \
             ON CONFLICT (org_id, kind) DO UPDATE SET \
             nonce = $3, secret = $4, account_label = $5, connected_by = $6, \
             created_at = ((extract(epoch FROM clock_timestamp()) * 1000000000)::bigint)",
        )
        .bind(org.as_uuid())
        .bind(connecting.provider.as_str())
        .bind(sealed.nonce())
        .bind(sealed.ciphertext())
        .bind(connecting.account_label)
        .bind(connecting.by.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|error| unreachable(&error, "write a connection"))?;
        Ok(())
    }

    async fn credential(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> Result<Option<ProviderToken>, ConnectionError> {
        let found = sqlx::query(
            "SELECT nonce AS nonce, secret AS secret FROM integrations \
             WHERE org_id = $1 AND kind = $2::integration_kind",
        )
        .bind(org.as_uuid())
        .bind(provider.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| unreachable(&error, "read a connection"))?;

        let Some(row) = found else {
            return Ok(None);
        };
        let sealed = Sealed::new(
            row.get::<Vec<u8>, _>("nonce"),
            row.get::<Vec<u8>, _>("secret"),
        );
        Ok(Some(self.vault.open(&sealed)?))
    }

    async fn disconnect(&self, org: TenantId, provider: Provider) -> Result<bool, ConnectionError> {
        sqlx::query("DELETE FROM integrations WHERE org_id = $1 AND kind = $2::integration_kind")
            .bind(org.as_uuid())
            .bind(provider.as_str())
            .execute(&self.pool)
            .await
            .map(|done| done.rows_affected() > 0)
            .map_err(|error| unreachable(&error, "drop a connection"))
    }
}

fn unreachable(error: &sqlx::Error, what: &str) -> ConnectionError {
    tracing::error!(%error, "the control plane refused to {what}");
    ConnectionError::StoreUnreachable
}
