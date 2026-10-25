use core::fmt;
use core::future::Future;

use crate::explore::ReadError;
use crate::identity::AccountId;
use crate::tenant::TenantId;
use crate::vault::{ProviderToken, SealError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Provider {
    Linear,
    Github,
    Jira,
}

impl Provider {
    /// The spellings the schema holds, which are also the ones the api and the screen carry.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Github => "github",
            Self::Jira => "jira",
        }
    }

    pub fn read(spelling: &str) -> Option<Self> {
        match spelling {
            "linear" => Some(Self::Linear),
            "github" => Some(Self::Github),
            "jira" => Some(Self::Jira),
            _ => None,
        }
    }

    /// Every one of them, so a screen can ask about the ones that are not connected too.
    pub fn every() -> [Self; 3] {
        [Self::Linear, Self::Github, Self::Jira]
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/* What a connection card shows, and not one field of it is a secret. The label is the workspace
or site the connection is to: "Connected" with nothing beside it says nothing worth reading, and
an organisation with two Jira sites needs to know which one this is. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub provider: Provider,
    pub account_label: Box<str>,
    pub connected_at_unix_nanos: u64,
    pub connected_by: Option<AccountId>,
}

/* The credential is handed about whole rather than as an access token, because the three
providers hand back different things — Jira a token, a refresh token and an expiry, the others
less — and the shape inside stays theirs to decide. What this layer guarantees is that it is
sealed on the way in and opened on the way out, and that the store never holds it in the clear. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connecting<'a> {
    pub provider: Provider,
    pub account_label: &'a str,
    pub credential: &'a ProviderToken,
    pub by: AccountId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConnectionError {
    #[error("a store could not be reached")]
    StoreUnreachable,
    #[error("this deployment holds no key to seal a connection with")]
    NoKey,
    /* The key changed, or the row came from another deployment. Worth its own answer because the
    screen can say the one useful thing about it: connect it again. */
    #[error("this connection does not open under this deployment's key")]
    Unopenable,
}

impl From<ReadError> for ConnectionError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::StoreUnreachable => Self::StoreUnreachable,
        }
    }
}

impl From<SealError> for ConnectionError {
    fn from(error: SealError) -> Self {
        match error {
            SealError::NoKey => Self::NoKey,
            SealError::Unsealable | SealError::Unopenable => Self::Unopenable,
        }
    }
}

pub trait Connections {
    /// What is connected, with nothing secret in it. The ones that are not connected are absent.
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Connection>, ConnectionError>> + Send;

    /// Connecting again replaces what was there, which is what reconnecting means.
    fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send;

    /* Opened, because the only caller is about to call the provider with it. Nothing else in the
    system reads this, and nothing logs it. */
    fn credential(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<ProviderToken>, ConnectionError>> + Send;

    /// Whether a connection was there to drop.
    fn disconnect(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<bool, ConnectionError>> + Send;
}

impl<T: Connections + Sync> Connections for &T {
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Connection>, ConnectionError>> + Send {
        (*self).of(org)
    }

    fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send {
        (*self).connect(org, connecting)
    }

    fn credential(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<ProviderToken>, ConnectionError>> + Send {
        (*self).credential(org, provider)
    }

    fn disconnect(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<bool, ConnectionError>> + Send {
        (*self).disconnect(org, provider)
    }
}

impl<T: Connections + Send + Sync> Connections for std::sync::Arc<T> {
    fn of(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Vec<Connection>, ConnectionError>> + Send {
        self.as_ref().of(org)
    }

    fn connect(
        &self,
        org: TenantId,
        connecting: Connecting<'_>,
    ) -> impl Future<Output = Result<(), ConnectionError>> + Send {
        self.as_ref().connect(org, connecting)
    }

    fn credential(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<ProviderToken>, ConnectionError>> + Send {
        self.as_ref().credential(org, provider)
    }

    fn disconnect(
        &self,
        org: TenantId,
        provider: Provider,
    ) -> impl Future<Output = Result<bool, ConnectionError>> + Send {
        self.as_ref().disconnect(org, provider)
    }
}

#[cfg(test)]
mod tests {
    use super::Provider;

    #[test]
    fn every_provider_survives_a_round_trip_through_its_spelling() {
        for provider in Provider::every() {
            assert_eq!(Provider::read(provider.as_str()), Some(provider));
        }
    }

    #[test]
    fn a_spelling_the_schema_does_not_hold_is_refused() {
        assert_eq!(Provider::read("gitlab"), None);
        assert_eq!(Provider::read("Linear"), None);
        assert_eq!(Provider::read(""), None);
    }

    /* The list the screen iterates to ask about the ones that are not connected. A provider
    added to the enum and forgotten here would simply never appear. */
    #[test]
    fn every_provider_is_in_the_list_of_all_of_them() {
        assert_eq!(
            Provider::every()
                .iter()
                .map(|one| one.as_str())
                .collect::<Vec<_>>(),
            vec!["linear", "github", "jira"]
        );
    }
}
