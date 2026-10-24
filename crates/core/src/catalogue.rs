use core::fmt;
use core::future::Future;

use uuid::Uuid;

use crate::explore::{ExploreError, ReadError};
use crate::membership::{Memberships, OrgRole, Whoami};
use crate::secret::SessionToken;
use crate::session::SessionStore;
use crate::tenant::TenantId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectId(Uuid);

impl ProjectId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub org: TenantId,
    pub name: Box<str>,
}

pub trait Projects {
    fn of(&self, tenant: TenantId) -> impl Future<Output = Result<Vec<Project>, ReadError>> + Send;
}

impl<T: Projects + Sync> Projects for &T {
    fn of(&self, tenant: TenantId) -> impl Future<Output = Result<Vec<Project>, ReadError>> + Send {
        (*self).of(tenant)
    }
}

impl<T: Projects + Send + Sync> Projects for std::sync::Arc<T> {
    fn of(&self, tenant: TenantId) -> impl Future<Output = Result<Vec<Project>, ReadError>> + Send {
        self.as_ref().of(tenant)
    }
}

/* The three answers this gives are the ones Explore gives, so it carries that error rather than a
second enum spelling them again. The name is wider than its module now; it wants renaming once a
third caller needs it. */
#[derive(Debug)]
pub struct Catalogue<S, G, P> {
    whoami: Whoami<S, G>,
    projects: P,
}

impl<S, G, P> Catalogue<S, G, P>
where
    S: SessionStore,
    G: Memberships,
    P: Projects,
{
    pub fn new(whoami: Whoami<S, G>, projects: P) -> Self {
        Self { whoami, projects }
    }

    pub async fn projects(
        &self,
        token: &SessionToken,
        org: TenantId,
    ) -> Result<Vec<Project>, ExploreError> {
        let who = self.whoami.of(token).await?;
        if !who.may_act_in(org, OrgRole::Member) {
            return Err(ExploreError::NotYours);
        }
        Ok(self.projects.of(org).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use uuid::Uuid;

    use super::{Catalogue, Project, ProjectId, Projects};
    use crate::clock::UnixSeconds;
    use crate::credential::Email;
    use crate::explore::{ExploreError, ReadError};
    use crate::identity::AccountId;
    use crate::membership::{Membership, Memberships, OrgRole, Whoami, WhoamiError};
    use crate::secret::SessionToken;
    use crate::session::{Session, SessionError, SessionStore};
    use crate::tenant::TenantId;

    const TOKEN: &str = "ct_ss_live";
    const MINE: u128 = 1;
    const THEIRS: u128 = 2;

    fn an_org(seed: u128) -> TenantId {
        TenantId::new(Uuid::from_u128(seed))
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

    struct MemberOfMine;

    impl Memberships for MemberOfMine {
        async fn of(&self, _account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
            Ok(vec![Membership {
                org: an_org(MINE),
                name: "Acme".into(),
                role: OrgRole::Member,
            }])
        }
    }

    #[derive(Default)]
    struct Asked {
        tenants: Mutex<Vec<TenantId>>,
    }

    impl Projects for &Asked {
        async fn of(&self, tenant: TenantId) -> Result<Vec<Project>, ReadError> {
            self.tenants.lock().expect("the lock").push(tenant);
            Ok(vec![Project {
                id: ProjectId::new(Uuid::from_u128(7)),
                org: tenant,
                name: "storefront".into(),
            }])
        }
    }

    fn cataloguing(asked: &Asked) -> Catalogue<OneSession, MemberOfMine, &Asked> {
        Catalogue::new(Whoami::new(OneSession, MemberOfMine), asked)
    }

    #[tokio::test]
    async fn a_member_reads_the_projects_of_its_own_organisation() {
        let asked = Asked::default();

        let found = cataloguing(&asked)
            .projects(&SessionToken::from_plaintext(TOKEN), an_org(MINE))
            .await
            .expect("a member may read");

        assert_eq!(found.len(), 1);
        assert_eq!(&*found[0].name, "storefront");
        assert_eq!(*asked.tenants.lock().expect("the lock"), vec![an_org(MINE)]);
    }

    #[tokio::test]
    async fn the_projects_of_another_organisation_are_refused_before_the_store_is_asked() {
        let asked = Asked::default();

        let refused = cataloguing(&asked)
            .projects(&SessionToken::from_plaintext(TOKEN), an_org(THEIRS))
            .await;

        assert_eq!(refused, Err(ExploreError::NotYours));
        assert!(asked.tenants.lock().expect("the lock").is_empty());
    }

    #[tokio::test]
    async fn a_token_no_session_carries_reads_nothing() {
        let asked = Asked::default();

        let refused = cataloguing(&asked)
            .projects(&SessionToken::from_plaintext("ct_ss_nope"), an_org(MINE))
            .await;

        assert_eq!(refused, Err(ExploreError::NoSession));
        assert!(asked.tenants.lock().expect("the lock").is_empty());
    }
}
