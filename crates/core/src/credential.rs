use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidCredential {
    #[error("an email address needs a local part and a domain")]
    Email,
    #[error("a subject cannot be empty")]
    Subject,
    #[error("an issuer cannot be empty")]
    Issuer,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Email(Box<str>);

impl Email {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Email {
    type Error = InvalidCredential;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        let trimmed = text.trim();
        let Some((local, domain)) = trimmed.split_once('@') else {
            return Err(InvalidCredential::Email);
        };
        let shaped = !local.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !trimmed.contains(char::is_whitespace);
        if shaped {
            Ok(Self(trimmed.to_ascii_lowercase().into_boxed_str()))
        } else {
            Err(InvalidCredential::Email)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Subject(Box<str>);

impl Subject {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Subject {
    type Error = InvalidCredential;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(InvalidCredential::Subject);
        }
        Ok(Self(trimmed.into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Issuer {
    Local,
    Federated(Box<str>),
}

impl Issuer {
    pub fn federated(url: &str) -> Result<Self, InvalidCredential> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(InvalidCredential::Issuer);
        }
        Ok(Self::Federated(trimmed.into()))
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Local => "local",
            Self::Federated(url) => url,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct Password(String);

impl Password {
    pub fn new(plaintext: &str) -> Self {
        Self(plaintext.to_owned())
    }

    pub fn exposed(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Password(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::{Email, InvalidCredential, Issuer, Password, Subject};

    #[test]
    fn an_email_is_lowercased_and_trimmed() {
        let email = Email::try_from("  Person@Example.COM ").expect("a shaped address");

        assert_eq!(email.as_str(), "person@example.com");
    }

    #[test]
    fn a_misshapen_email_is_refused() {
        for candidate in [
            "person",
            "@example.com",
            "person@example",
            "person@.com",
            "person@example.",
            "per son@example.com",
        ] {
            assert_eq!(
                Email::try_from(candidate),
                Err(InvalidCredential::Email),
                "{candidate} should be refused"
            );
        }
    }

    #[test]
    fn a_subject_cannot_be_blank() {
        assert_eq!(Subject::try_from("   "), Err(InvalidCredential::Subject));
        assert_eq!(
            Subject::try_from(" sub-123 ").map(|s| s.as_str().to_owned()),
            Ok("sub-123".to_owned())
        );
    }

    #[test]
    fn the_local_issuer_is_not_a_url() {
        assert!(Issuer::Local.is_local());
        assert_eq!(Issuer::Local.as_str(), "local");

        let federated = Issuer::federated("https://id.example").expect("a url");
        assert!(!federated.is_local());
        assert_eq!(federated.as_str(), "https://id.example");
        assert_eq!(Issuer::federated("  "), Err(InvalidCredential::Issuer));
    }

    #[test]
    fn a_password_never_renders_itself() {
        let password = Password::new("correct horse battery");

        assert_eq!(format!("{password:?}"), "Password(redacted)");
        assert_eq!(password.exposed(), "correct horse battery");
    }
}
