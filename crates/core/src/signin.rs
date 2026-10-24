use core::future::Future;

use crate::identity::{
    AccountId, AuthError, AuthProvider, AuthSources, ProvisionPolicy, ResolvedIdentity, SignInProof,
};
use crate::secret::SessionToken;
use crate::session::{Session, SessionError, SessionStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignInError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Session(#[from] SessionError),
}

pub trait AccountDirectory {
    fn provision(
        &self,
        identity: &ResolvedIdentity,
        policy: ProvisionPolicy,
    ) -> impl Future<Output = Result<AccountId, AuthError>> + Send;
}

impl<T: AccountDirectory + Sync> AccountDirectory for &T {
    fn provision(
        &self,
        identity: &ResolvedIdentity,
        policy: ProvisionPolicy,
    ) -> impl Future<Output = Result<AccountId, AuthError>> + Send {
        (*self).provision(identity, policy)
    }
}

#[derive(Debug)]
pub struct SignIn<L, F, A, S> {
    sources: AuthSources<L, F>,
    accounts: A,
    sessions: S,
    policy: ProvisionPolicy,
}

impl<L, F, A, S> SignIn<L, F, A, S>
where
    L: AuthProvider,
    F: AuthProvider,
    A: AccountDirectory,
    S: SessionStore,
{
    pub fn new(
        sources: AuthSources<L, F>,
        accounts: A,
        sessions: S,
        policy: ProvisionPolicy,
    ) -> Self {
        Self {
            sources,
            accounts,
            sessions,
            policy,
        }
    }

    pub fn local_enabled(&self) -> bool {
        self.sources.local_enabled()
    }

    pub fn federated_enabled(&self) -> bool {
        self.sources.federated_enabled()
    }

    pub async fn begin(&self, proof: SignInProof) -> Result<SessionToken, SignInError> {
        let identity = self.sources.resolve(proof).await?;
        let account = self.accounts.provision(&identity, self.policy).await?;
        let token = self.sessions.issue(account, None).await?;
        Ok(token)
    }

    pub async fn resolve(&self, token: &SessionToken) -> Result<Session, SignInError> {
        Ok(self.sessions.resolve(token).await?)
    }

    pub async fn end(&self, token: &SessionToken) -> Result<(), SignInError> {
        Ok(self.sessions.revoke(token).await?)
    }
}
