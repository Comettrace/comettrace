use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};
use comettrace_core::{ProviderToken, SealError, Sealed, Vault};

/// Thirty-two bytes, written as the sixty-four hex characters an operator pastes into a setting.
const KEY_BYTES: usize = 32;

/// What `XChaCha20` takes, and the whole reason it is the one chosen: wide enough to draw at random.
const NONCE_BYTES: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadKey {
    #[error("a deployment key is {KEY_BYTES} bytes, which is {} hex characters", KEY_BYTES * 2)]
    Length,
    #[error("a deployment key is hex, and this is not")]
    NotHex,
}

/* XChaCha20-Poly1305 rather than its narrower sibling, for one reason: the nonce is twenty-four
bytes, so it can be drawn at random for every sealing and never thought about again. A twelve
byte nonce would need a counter kept somewhere, and a counter that two server processes share is
a thing to get wrong — reuse a nonce under the same key and the construction stops protecting
anything.

Poly1305 is the half that matters most here. It is what makes `open` able to fail: a row somebody
edited by hand, or a ciphertext moved from one deployment to another, is refused rather than
decrypted into nonsense that gets sent to Linear as a bearer token. */
#[derive(Clone)]
pub struct SealedVault {
    cipher: XChaCha20Poly1305,
}

impl SealedVault {
    pub fn from_hex_key(key: &str) -> Result<Self, BadKey> {
        let bytes = hex::decode(key.trim()).map_err(|_| BadKey::NotHex)?;
        if bytes.len() != KEY_BYTES {
            return Err(BadKey::Length);
        }
        Ok(Self {
            cipher: XChaCha20Poly1305::new(bytes.as_slice().into()),
        })
    }

    /// Sixty-four hex characters an operator can paste into a setting, drawn here so nobody has
    /// to go looking for a way to make thirty-two random bytes.
    pub fn fresh_key() -> String {
        let bytes: [u8; KEY_BYTES] = rand::random();
        hex::encode(bytes)
    }
}

/* The key never renders. Without this a deployment's whole secret could reach a log line through
any struct that happens to hold the vault and derives Debug. */
impl core::fmt::Debug for SealedVault {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("SealedVault(key withheld)")
    }
}

impl Vault for SealedVault {
    fn seal(&self, secret: &ProviderToken) -> Result<Sealed, SealError> {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, secret.exposed().as_bytes())
            .map_err(|_| SealError::Unsealable)?;
        Ok(Sealed::new(nonce.to_vec(), ciphertext))
    }

    fn open(&self, sealed: &Sealed) -> Result<ProviderToken, SealError> {
        /* Copied into a fixed array rather than borrowed as one, so a row carrying a nonce of the
        wrong length is refused here instead of asserting its way out of the process. */
        let Ok(nonce) = <[u8; NONCE_BYTES]>::try_from(sealed.nonce()) else {
            return Err(SealError::Unopenable);
        };
        let plaintext = self
            .cipher
            .decrypt(&XNonce::from(nonce), sealed.ciphertext())
            .map_err(|_| SealError::Unopenable)?;
        let text = String::from_utf8(plaintext).map_err(|_| SealError::Unopenable)?;
        Ok(ProviderToken::new(&text))
    }
}

#[cfg(test)]
mod tests {
    use comettrace_core::{ProviderToken, SealError, Sealed, Vault};

    use super::{BadKey, SealedVault};

    const A_TOKEN: &str = "a-token-somebody-else-issued";

    fn a_vault() -> SealedVault {
        SealedVault::from_hex_key(&SealedVault::fresh_key()).expect("a fresh key is a good one")
    }

    #[test]
    fn a_sealed_token_opens_back_into_itself() {
        let vault = a_vault();

        let sealed = vault
            .seal(&ProviderToken::new(A_TOKEN))
            .expect("the sealing");

        assert_eq!(vault.open(&sealed).expect("the opening").exposed(), A_TOKEN);
    }

    /* The whole point of a fresh nonce: the same secret under the same key must not produce the
    same row twice, or the database says which organisations share a token. */
    #[test]
    fn sealing_the_same_token_twice_gives_two_different_rows() {
        let vault = a_vault();
        let token = ProviderToken::new(A_TOKEN);

        let once = vault.seal(&token).expect("the first sealing");
        let twice = vault.seal(&token).expect("the second sealing");

        assert_ne!(once.nonce(), twice.nonce());
        assert_ne!(once.ciphertext(), twice.ciphertext());
        assert_eq!(
            vault.open(&twice).expect("the opening").exposed(),
            A_TOKEN,
            "and both still open"
        );
    }

    #[test]
    fn a_row_sealed_under_another_key_does_not_open() {
        let sealed = a_vault()
            .seal(&ProviderToken::new(A_TOKEN))
            .expect("the sealing");

        let refused = a_vault().open(&sealed);

        assert_eq!(refused.err(), Some(SealError::Unopenable));
    }

    /* The property Poly1305 is here for. A ciphertext somebody edited must be refused, not
    decrypted into nonsense that would go out as a bearer token. */
    #[test]
    fn a_ciphertext_somebody_edited_is_refused_rather_than_opened() {
        let vault = a_vault();
        let sealed = vault
            .seal(&ProviderToken::new(A_TOKEN))
            .expect("the sealing");
        let mut tampered = sealed.ciphertext().to_vec();
        tampered[0] ^= 0x01;

        let refused = vault.open(&Sealed::new(sealed.nonce().to_vec(), tampered));

        assert_eq!(refused.err(), Some(SealError::Unopenable));
    }

    #[test]
    fn a_nonce_somebody_edited_is_refused_too() {
        let vault = a_vault();
        let sealed = vault
            .seal(&ProviderToken::new(A_TOKEN))
            .expect("the sealing");
        let mut tampered = sealed.nonce().to_vec();
        tampered[0] ^= 0x01;

        let refused = vault.open(&Sealed::new(tampered, sealed.ciphertext().to_vec()));

        assert_eq!(refused.err(), Some(SealError::Unopenable));
    }

    /* A truncated nonce would panic `from_slice`, and a key that is half typed is the likeliest
    way anybody meets this code. Both answer rather than abort. */
    #[test]
    fn a_nonce_of_the_wrong_length_is_refused_rather_than_fatal() {
        let refused = a_vault().open(&Sealed::new(vec![0u8; 8], vec![7u8; 48]));

        assert_eq!(refused.err(), Some(SealError::Unopenable));
    }

    #[test]
    fn a_key_is_sixty_four_hex_characters_and_nothing_else() {
        assert_eq!(SealedVault::fresh_key().len(), 64);
        assert!(SealedVault::from_hex_key(&SealedVault::fresh_key()).is_ok());
        assert_eq!(
            SealedVault::from_hex_key(&"ab".repeat(16)).err(),
            Some(BadKey::Length),
            "thirty-two hex characters is sixteen bytes"
        );
        assert_eq!(
            SealedVault::from_hex_key(&"zz".repeat(32)).err(),
            Some(BadKey::NotHex)
        );
    }

    /* Whitespace is what a copied line brings with it, and a key that fails for a trailing
    newline is an hour of somebody's evening. */
    #[test]
    fn a_key_survives_the_newline_a_paste_brings_with_it() {
        let key = SealedVault::fresh_key();

        assert!(SealedVault::from_hex_key(&format!("  {key}\n")).is_ok());
    }

    #[test]
    fn the_key_never_reaches_a_log_line() {
        let rendered = format!("{:?}", a_vault());

        assert_eq!(rendered, "SealedVault(key withheld)");
    }
}
