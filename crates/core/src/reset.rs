use core::future::Future;

use crate::credential::{Email, Password};
use crate::identity::AccountId;
use crate::secret::{ResetToken, SecretHash};
use crate::session::{HashError, PasswordHash, PasswordHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResetError {
    #[error("no live reset carries this token")]
    Unknown,
    #[error("the control plane could not be reached")]
    StoreUnreachable,
    #[error("the message could not be sent")]
    Undeliverable,
    #[error(transparent)]
    Hashing(#[from] HashError),
}

pub trait ResetStore {
    fn account_for(
        &self,
        email: &Email,
    ) -> impl Future<Output = Result<Option<AccountId>, ResetError>> + Send;

    fn begin(
        &self,
        account: AccountId,
        digest: SecretHash,
    ) -> impl Future<Output = Result<(), ResetError>> + Send;

    fn take(
        &self,
        digest: SecretHash,
    ) -> impl Future<Output = Result<AccountId, ResetError>> + Send;

    fn set_password(
        &self,
        account: AccountId,
        hash: &PasswordHash,
    ) -> impl Future<Output = Result<(), ResetError>> + Send;

    fn revoke_sessions(
        &self,
        account: AccountId,
    ) -> impl Future<Output = Result<u64, ResetError>> + Send;
}

pub trait Mailer {
    fn send_password_reset(
        &self,
        to: &Email,
        token: &ResetToken,
    ) -> impl Future<Output = Result<(), ResetError>> + Send;
}

#[derive(Debug)]
pub struct ResetPassword<R, H, M> {
    store: R,
    hasher: H,
    mailer: M,
}

impl<R, H, M> ResetPassword<R, H, M>
where
    R: ResetStore,
    H: PasswordHasher,
    M: Mailer,
{
    pub fn new(store: R, hasher: H, mailer: M) -> Self {
        Self {
            store,
            hasher,
            mailer,
        }
    }

    pub async fn request(&self, email: &Email, token: ResetToken) -> Result<bool, ResetError> {
        let Some(account) = self.store.account_for(email).await? else {
            return Ok(false);
        };
        self.store.begin(account, token.hash()).await?;
        self.mailer.send_password_reset(email, &token).await?;
        Ok(true)
    }

    pub async fn confirm(&self, token: &ResetToken, chosen: &Password) -> Result<u64, ResetError> {
        let account = self.store.take(token.hash()).await?;
        let hash = self.hasher.hash(chosen).await?;
        self.store.set_password(account, &hash).await?;
        self.store.revoke_sessions(account).await
    }
}
