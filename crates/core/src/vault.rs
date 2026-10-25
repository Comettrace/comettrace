use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

/* A secret somebody else issued, which is a kind this system has not had to hold until now.
Every secret here so far has been one it could forget: a password is hashed and never read back,
an ingest key is hashed and compared against, a session token is a digest in a row. An access
token from Linear has to go back out on the next request, so it has to come back — which is why
it is sealed rather than hashed, and why there is a key to lose. */
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct ProviderToken(String);

impl ProviderToken {
    pub fn new(plaintext: &str) -> Self {
        Self(plaintext.to_owned())
    }

    pub fn exposed(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProviderToken(redacted)")
    }
}

/* What goes in a row: a nonce drawn afresh for every sealing, and the ciphertext with its
authentication tag on the end. The tag is what makes opening a sealed secret able to fail rather
than able to return nonsense — a row somebody edited in the database does not decrypt, it is
refused. */
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed {
    nonce: Box<[u8]>,
    ciphertext: Box<[u8]>,
}

impl Sealed {
    pub fn new(nonce: impl Into<Box<[u8]>>, ciphertext: impl Into<Box<[u8]>>) -> Self {
        Self {
            nonce: nonce.into(),
            ciphertext: ciphertext.into(),
        }
    }

    pub fn nonce(&self) -> &[u8] {
        &self.nonce
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/* The lengths rather than the bytes. Ciphertext is not a secret, but a log line carrying it is a
log line somebody keeps for a year beside the key, and the nonce says which row this is. */
impl fmt::Debug for Sealed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Sealed({} bytes under a {} byte nonce)",
            self.ciphertext.len(),
            self.nonce.len()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    #[error("this deployment holds no key to seal a secret with")]
    NoKey,
    #[error("the secret could not be sealed")]
    Unsealable,
    #[error("the sealed secret does not open under this deployment's key")]
    Unopenable,
}

/* Synchronous, unlike the password hasher. Hashing a password is deliberately expensive and goes
off the runtime; sealing a token is a few microseconds, and an async port for it would buy a
`.await` and nothing else. */
pub trait Vault {
    fn seal(&self, secret: &ProviderToken) -> Result<Sealed, SealError>;

    fn open(&self, sealed: &Sealed) -> Result<ProviderToken, SealError>;
}

impl<T: Vault> Vault for &T {
    fn seal(&self, secret: &ProviderToken) -> Result<Sealed, SealError> {
        (*self).seal(secret)
    }

    fn open(&self, sealed: &Sealed) -> Result<ProviderToken, SealError> {
        (*self).open(sealed)
    }
}

impl<T: Vault> Vault for std::sync::Arc<T> {
    fn seal(&self, secret: &ProviderToken) -> Result<Sealed, SealError> {
        self.as_ref().seal(secret)
    }

    fn open(&self, sealed: &Sealed) -> Result<ProviderToken, SealError> {
        self.as_ref().open(sealed)
    }
}

/* A deployment that set no key can still do everything it did before; what it cannot do is hold
a token for somebody else's service. Every route that would need one is told so by this rather
than by a panic at startup, because a deployment with no project management connected is the
ordinary case and not a misconfiguration. */
#[derive(Debug, Clone, Copy, Default)]
pub struct NoVault;

impl Vault for NoVault {
    fn seal(&self, _secret: &ProviderToken) -> Result<Sealed, SealError> {
        Err(SealError::NoKey)
    }

    fn open(&self, _sealed: &Sealed) -> Result<ProviderToken, SealError> {
        Err(SealError::NoKey)
    }
}

#[cfg(test)]
mod tests {
    use super::{NoVault, ProviderToken, SealError, Sealed, Vault};

    #[test]
    fn neither_a_token_nor_a_sealed_secret_renders_itself() {
        let token = ProviderToken::new("lin_api_the_real_thing");
        let sealed = Sealed::new(vec![0u8; 24], vec![7u8; 48]);

        assert_eq!(format!("{token:?}"), "ProviderToken(redacted)");
        assert!(!format!("{token:?}").contains("lin_api"));
        assert_eq!(
            format!("{sealed:?}"),
            "Sealed(48 bytes under a 24 byte nonce)"
        );
    }

    /* A deployment with no key is not broken, it is unconnected, and both directions have to say
    the same thing about it. */
    #[test]
    fn a_deployment_with_no_key_seals_and_opens_nothing() {
        let token = ProviderToken::new("lin_api_the_real_thing");

        assert_eq!(NoVault.seal(&token), Err(SealError::NoKey));
        assert_eq!(
            NoVault.open(&Sealed::new(vec![0u8; 24], vec![7u8; 48])),
            Err(SealError::NoKey)
        );
    }
}
