use comettrace_core::{
    AccountDirectory, AccountId, AuthError, Email, PasswordHash, ProvisionPolicy, ResolvedIdentity,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("the control plane refused the account query")]
    ControlPlane(#[from] sqlx::Error),
    #[error("this identity carries no account and provisioning is off")]
    NotProvisioned,
    #[error("an account already carries this address")]
    Taken,
}

#[derive(Debug, Clone)]
pub struct PostgresAccounts {
    pool: PgPool,
}

impl PostgresAccounts {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        email: &Email,
        locked: &PasswordHash,
    ) -> Result<AccountId, AccountError> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(email.as_str())
            .bind(locked.encoded())
            .execute(&self.pool)
            .await
            .map_err(|error| match &error {
                sqlx::Error::Database(refusal) if refusal.is_unique_violation() => {
                    AccountError::Taken
                }
                _ => AccountError::ControlPlane(error),
            })?;
        Ok(AccountId::new(id))
    }

    pub async fn linked(
        &self,
        identity: &ResolvedIdentity,
    ) -> Result<Option<AccountId>, AccountError> {
        let found = sqlx::query(
            "SELECT account_id AS account_id FROM identity_links \
             WHERE issuer = $1 AND subject = $2",
        )
        .bind(identity.issuer.as_str())
        .bind(identity.subject.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(found.map(|row| AccountId::new(row.get("account_id"))))
    }

    pub async fn provision(
        &self,
        identity: &ResolvedIdentity,
        policy: ProvisionPolicy,
    ) -> Result<AccountId, AccountError> {
        if let Some(account) = self.linked(identity).await? {
            return Ok(account);
        }

        let mut transaction = self.pool.begin().await?;

        let existing: Option<Uuid> = if policy.link_by_email {
            sqlx::query("SELECT id AS id FROM accounts WHERE email = $1")
                .bind(identity.email.as_str())
                .fetch_optional(&mut *transaction)
                .await?
                .map(|row| row.get("id"))
        } else {
            None
        };

        let account = match existing {
            Some(account) => account,
            None if policy.auto_provision => {
                let account = Uuid::new_v4();
                sqlx::query("INSERT INTO accounts (id, email) VALUES ($1, $2)")
                    .bind(account)
                    .bind(identity.email.as_str())
                    .execute(&mut *transaction)
                    .await?;
                account
            }
            None => return Err(AccountError::NotProvisioned),
        };

        sqlx::query("INSERT INTO identity_links (issuer, subject, account_id) VALUES ($1, $2, $3)")
            .bind(identity.issuer.as_str())
            .bind(identity.subject.as_str())
            .bind(account)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;
        Ok(AccountId::new(account))
    }
}

impl AccountDirectory for PostgresAccounts {
    async fn provision(
        &self,
        identity: &ResolvedIdentity,
        policy: ProvisionPolicy,
    ) -> Result<AccountId, AuthError> {
        match PostgresAccounts::provision(self, identity, policy).await {
            Ok(account) => Ok(account),
            Err(AccountError::NotProvisioned) => Err(AuthError::Rejected),
            Err(error) => {
                tracing::error!(%error, "the control plane refused to provision an account");
                Err(AuthError::ProviderUnreachable)
            }
        }
    }
}
