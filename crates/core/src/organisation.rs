use core::future::Future;

use crate::explore::ReadError;
use crate::membership::{Memberships, OrgRole, Whoami, WhoamiError};
use crate::secret::SessionToken;
use crate::session::SessionStore;
use crate::tenant::TenantId;

/* The ceiling counts bytes because that is what the column holds; the name is trimmed first
because surrounding space is invisible in the top bar and on an invitation, and two organisations
that differ only by it would look like one. */
const MAX_NAME_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadName {
    #[error("an organisation needs a name")]
    Blank,
    #[error("an organisation name is at most {0} bytes")]
    TooLong(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgName(Box<str>);

impl OrgName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for OrgName {
    type Error = BadName;

    fn try_from(chosen: &str) -> Result<Self, Self::Error> {
        let kept = chosen.trim();
        if kept.len() > MAX_NAME_BYTES {
            return Err(BadName::TooLong(MAX_NAME_BYTES));
        }
        if kept.is_empty() {
            return Err(BadName::Blank);
        }
        Ok(Self(kept.into()))
    }
}

/* What the settings screen calls the organisation card. The version is a fact about the binary
rather than about the organisation, and it rides here because the screen shows it in the same
table: a route and a query of its own would carry two constants. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organisation {
    pub id: TenantId,
    pub name: Box<str>,
    pub plan: Box<str>,
    pub created_at_unix_nanos: u64,
}

pub trait Orgs {
    fn only(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Option<Organisation>, ReadError>> + Send;

    fn rename(
        &self,
        org: TenantId,
        name: &OrgName,
    ) -> impl Future<Output = Result<(), ReadError>> + Send;
}

impl<T: Orgs + Sync> Orgs for &T {
    fn only(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Option<Organisation>, ReadError>> + Send {
        (*self).only(org)
    }

    fn rename(
        &self,
        org: TenantId,
        name: &OrgName,
    ) -> impl Future<Output = Result<(), ReadError>> + Send {
        (*self).rename(org, name)
    }
}

impl<T: Orgs + Send + Sync> Orgs for std::sync::Arc<T> {
    fn only(
        &self,
        org: TenantId,
    ) -> impl Future<Output = Result<Option<Organisation>, ReadError>> + Send {
        self.as_ref().only(org)
    }

    fn rename(
        &self,
        org: TenantId,
        name: &OrgName,
    ) -> impl Future<Output = Result<(), ReadError>> + Send {
        self.as_ref().rename(org, name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SettingError {
    #[error("no live session carries this token")]
    NoSession,
    #[error("this session holds no membership of that organisation")]
    NotYours,
    #[error("changing an organisation is an administrator's to do")]
    NotYoursToChange,
    #[error("no organisation carries that id any more")]
    Gone,
    #[error("a store could not be reached")]
    StoreUnreachable,
}

impl From<WhoamiError> for SettingError {
    fn from(error: WhoamiError) -> Self {
        match error {
            WhoamiError::NoSession => Self::NoSession,
            WhoamiError::StoreUnreachable => Self::StoreUnreachable,
        }
    }
}

impl From<ReadError> for SettingError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::StoreUnreachable => Self::StoreUnreachable,
        }
    }
}

#[derive(Debug)]
pub struct OrgSettings<S, G, O> {
    whoami: Whoami<S, G>,
    orgs: O,
}

impl<S, G, O> OrgSettings<S, G, O>
where
    S: SessionStore,
    G: Memberships,
    O: Orgs,
{
    pub fn new(whoami: Whoami<S, G>, orgs: O) -> Self {
        Self { whoami, orgs }
    }

    pub async fn read(
        &self,
        token: &SessionToken,
        org: TenantId,
    ) -> Result<Organisation, SettingError> {
        self.standing(token, org, OrgRole::Member).await?;
        self.card(org).await
    }

    /* The card is read back rather than assembled from what was asked for: the trimming the name
    went through is what the screen should show, and so is a change an administrator made in
    another tab a moment ago. */
    pub async fn rename(
        &self,
        token: &SessionToken,
        org: TenantId,
        name: &OrgName,
    ) -> Result<Organisation, SettingError> {
        self.standing(token, org, OrgRole::Admin).await?;
        self.orgs.rename(org, name).await?;
        self.card(org).await
    }

    async fn standing(
        &self,
        token: &SessionToken,
        org: TenantId,
        floor: OrgRole,
    ) -> Result<(), SettingError> {
        let who = self.whoami.of(token).await?;
        match who.role_in(org) {
            None => Err(SettingError::NotYours),
            Some(role) if !role.at_least(floor) => Err(SettingError::NotYoursToChange),
            Some(_) => Ok(()),
        }
    }

    async fn card(&self, org: TenantId) -> Result<Organisation, SettingError> {
        self.orgs.only(org).await?.ok_or(SettingError::Gone)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use uuid::Uuid;

    use super::{BadName, MAX_NAME_BYTES, OrgName, OrgSettings, Organisation, Orgs, SettingError};
    use crate::clock::UnixSeconds;
    use crate::credential::Email;
    use crate::explore::ReadError;
    use crate::identity::AccountId;
    use crate::membership::{Membership, Memberships, OrgRole, Whoami, WhoamiError};
    use crate::secret::SessionToken;
    use crate::session::{Session, SessionError, SessionStore};
    use crate::tenant::TenantId;

    const TOKEN: &str = "ct_ss_live";
    const MINE: u128 = 1;
    const THEIRS: u128 = 2;
    const FOUNDED: u64 = 1_700_000_000_000_000_000;

    fn an_org(seed: u128) -> TenantId {
        TenantId::new(Uuid::from_u128(seed))
    }

    fn a_name(spelling: &str) -> OrgName {
        OrgName::try_from(spelling).expect("an admitted name")
    }

    struct OneSession;

    impl SessionStore for OneSession {
        async fn issue(
            &self,
            _account: AccountId,
            _org: Option<TenantId>,
        ) -> Result<SessionToken, SessionError> {
            Ok(SessionToken::from_plaintext(TOKEN))
        }

        async fn resolve(&self, token: &SessionToken) -> Result<Session, SessionError> {
            if token.exposed() == TOKEN {
                Ok(Session {
                    account: AccountId::new(Uuid::from_u128(9)),
                    email: Email::try_from("person@example.com").expect("an address"),
                    org: None,
                    expires_at: UnixSeconds::new(0),
                })
            } else {
                Err(SessionError::Unknown)
            }
        }

        async fn revoke(&self, _token: &SessionToken) -> Result<(), SessionError> {
            Ok(())
        }
    }

    struct MemberOfMine(OrgRole);

    impl Memberships for MemberOfMine {
        async fn of(&self, _account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
            Ok(vec![Membership {
                org: an_org(MINE),
                name: "Acme".into(),
                role: self.0,
            }])
        }
    }

    #[derive(Debug)]
    struct Kept {
        name: Mutex<Option<String>>,
        renames: Mutex<Vec<String>>,
    }

    impl Kept {
        fn holding(name: &str) -> Self {
            Self {
                name: Mutex::new(Some(name.to_owned())),
                renames: Mutex::default(),
            }
        }

        fn gone() -> Self {
            Self {
                name: Mutex::new(None),
                renames: Mutex::default(),
            }
        }

        fn renames(&self) -> Vec<String> {
            self.renames.lock().expect("the lock").clone()
        }
    }

    impl Orgs for &Kept {
        async fn only(&self, org: TenantId) -> Result<Option<Organisation>, ReadError> {
            Ok(self
                .name
                .lock()
                .expect("the lock")
                .clone()
                .map(|name| Organisation {
                    id: org,
                    name: name.into(),
                    plan: "Self-hosted".into(),
                    created_at_unix_nanos: FOUNDED,
                }))
        }

        async fn rename(&self, _org: TenantId, name: &OrgName) -> Result<(), ReadError> {
            self.renames
                .lock()
                .expect("the lock")
                .push(name.as_str().to_owned());
            *self.name.lock().expect("the lock") = Some(name.as_str().to_owned());
            Ok(())
        }
    }

    fn settings(role: OrgRole, kept: &Kept) -> OrgSettings<OneSession, MemberOfMine, &Kept> {
        OrgSettings::new(Whoami::new(OneSession, MemberOfMine(role)), kept)
    }

    fn token() -> SessionToken {
        SessionToken::from_plaintext(TOKEN)
    }

    #[tokio::test]
    async fn a_member_reads_the_card_of_its_own_organisation() {
        let kept = Kept::holding("Acme");

        let card = settings(OrgRole::Member, &kept)
            .read(&token(), an_org(MINE))
            .await
            .expect("a member may read");

        assert_eq!(
            card,
            Organisation {
                id: an_org(MINE),
                name: "Acme".into(),
                plan: "Self-hosted".into(),
                created_at_unix_nanos: FOUNDED,
            }
        );
    }

    #[tokio::test]
    async fn the_card_of_another_organisation_is_refused() {
        let kept = Kept::holding("Acme");

        let refused = settings(OrgRole::Owner, &kept)
            .read(&token(), an_org(THEIRS))
            .await;

        assert_eq!(refused, Err(SettingError::NotYours));
    }

    #[tokio::test]
    async fn a_token_no_session_carries_reads_nothing() {
        let kept = Kept::holding("Acme");

        let refused = settings(OrgRole::Owner, &kept)
            .read(&SessionToken::from_plaintext("ct_ss_nope"), an_org(MINE))
            .await;

        assert_eq!(refused, Err(SettingError::NoSession));
    }

    #[tokio::test]
    async fn an_administrator_renames_it_and_reads_the_new_name_back() {
        let kept = Kept::holding("Acme");

        let card = settings(OrgRole::Admin, &kept)
            .rename(&token(), an_org(MINE), &a_name("Acme Payments"))
            .await
            .expect("an administrator may rename");

        assert_eq!(&*card.name, "Acme Payments");
        assert_eq!(kept.renames(), vec!["Acme Payments".to_owned()]);
    }

    #[tokio::test]
    async fn a_member_cannot_rename_it_and_the_store_is_never_asked() {
        let kept = Kept::holding("Acme");

        let refused = settings(OrgRole::Member, &kept)
            .rename(&token(), an_org(MINE), &a_name("Mine Now"))
            .await;

        assert_eq!(refused, Err(SettingError::NotYoursToChange));
        assert!(kept.renames().is_empty());
    }

    #[tokio::test]
    async fn renaming_an_organisation_this_session_is_no_part_of_says_no_such_thing() {
        let kept = Kept::holding("Acme");

        let refused = settings(OrgRole::Owner, &kept)
            .rename(&token(), an_org(THEIRS), &a_name("Theirs Now"))
            .await;

        assert_eq!(refused, Err(SettingError::NotYours));
        assert!(kept.renames().is_empty());
    }

    #[tokio::test]
    async fn an_organisation_the_membership_outlived_is_gone_rather_than_blank() {
        let kept = Kept::gone();

        let refused = settings(OrgRole::Owner, &kept)
            .read(&token(), an_org(MINE))
            .await;

        assert_eq!(refused, Err(SettingError::Gone));
    }

    #[test]
    fn a_name_is_kept_without_the_space_around_it() {
        assert_eq!(a_name("  Acme Payments \n").as_str(), "Acme Payments");
    }

    #[test]
    fn a_name_that_is_only_space_is_as_blank_as_an_empty_one() {
        assert_eq!(OrgName::try_from(""), Err(BadName::Blank));
        assert_eq!(OrgName::try_from("   \t\n"), Err(BadName::Blank));
    }

    #[test]
    fn a_name_past_the_ceiling_is_refused() {
        let long = "a".repeat(MAX_NAME_BYTES + 1);

        assert_eq!(
            OrgName::try_from(long.as_str()),
            Err(BadName::TooLong(MAX_NAME_BYTES))
        );
        assert!(OrgName::try_from("a".repeat(MAX_NAME_BYTES).as_str()).is_ok());
    }

    /* The ceiling is about the column, so a name well inside it in characters can still be past
    it in bytes. That is the intent, not an oversight. */
    #[test]
    fn the_ceiling_counts_bytes_rather_than_characters() {
        let wide = "\u{e9}".repeat(MAX_NAME_BYTES / 2 + 1);

        assert!(wide.chars().count() < MAX_NAME_BYTES);
        assert_eq!(
            OrgName::try_from(wide.as_str()),
            Err(BadName::TooLong(MAX_NAME_BYTES))
        );
    }
}
