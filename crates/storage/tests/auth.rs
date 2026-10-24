use comettrace_core::{
    AccountId, AuthError, AuthProvider, Email, Password, PasswordCheck, PasswordHasher,
    SessionError, SessionLifetime, SessionStore, SignInProof,
};
use comettrace_storage::{
    Argon2Hasher, CONTROL_PLANE, FixedClock, LocalPasswordProvider, PostgresSessions,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool, Row};
use uuid::Uuid;

const NOW: u64 = 1_700_000_000;

fn cheap() -> Argon2Hasher {
    Argon2Hasher::with_cost(8, 1, 1).expect("a cheap hasher")
}

fn dearer() -> Argon2Hasher {
    Argon2Hasher::with_cost(32, 2, 1).expect("a dearer hasher")
}

fn lifetime() -> SessionLifetime {
    SessionLifetime {
        idle_seconds: 100,
        hard_seconds: 1000,
    }
}

async fn control_plane(schema: &str) -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connecting to postgres");
    sqlx::query(AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&pool)
    .await
    .expect("dropping a leftover schema");
    sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&pool)
        .await
        .expect("creating the test schema");
    pool.close().await;

    let owned = schema.to_owned();
    let scoped = PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _| {
            let schema = owned.clone();
            Box::pin(async move {
                sqlx::query(AssertSqlSafe(format!("SET search_path TO {schema}")))
                    .execute(connection)
                    .await
                    .map(|_| ())
            })
        })
        .connect(&url)
        .await
        .expect("connecting to the test schema");
    CONTROL_PLANE
        .run(&scoped)
        .await
        .expect("the migrations apply");
    Some(scoped)
}

async fn an_account(pool: &PgPool, email: &str, password: Option<&str>) -> AccountId {
    let id = Uuid::new_v4();
    let hash = match password {
        Some(plaintext) => Some(
            cheap()
                .hash(&Password::new(plaintext))
                .await
                .expect("a hash")
                .encoded()
                .to_owned(),
        ),
        None => None,
    };
    sqlx::query("INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(email)
        .bind(hash)
        .execute(pool)
        .await
        .expect("an account");
    AccountId::new(id)
}

fn a_proof(email: &str, password: &str) -> SignInProof {
    SignInProof::Password {
        email: Email::try_from(email).expect("an address"),
        password: Password::new(password),
    }
}

#[tokio::test]
async fn a_hash_accepts_its_own_password() {
    let hasher = cheap();
    let password = Password::new("correct horse battery");

    let hash = hasher.hash(&password).await.expect("a hash");
    let check = hasher.check(&password, &hash).await.expect("a check");

    assert_eq!(check, PasswordCheck::Accepted);
    assert!(
        hash.encoded().starts_with("$argon2id$"),
        "{}",
        hash.encoded()
    );
}

#[tokio::test]
async fn a_wrong_password_is_rejected() {
    let hasher = cheap();
    let hash = hasher
        .hash(&Password::new("correct horse battery"))
        .await
        .expect("a hash");

    let check = hasher
        .check(&Password::new("wrong horse battery"), &hash)
        .await
        .expect("a check");

    assert_eq!(check, PasswordCheck::Rejected);
}

#[tokio::test]
async fn the_decoy_matches_nothing() {
    let hasher = cheap();

    let check = hasher
        .check(&Password::new("anything at all"), hasher.decoy())
        .await
        .expect("a check");

    assert_eq!(check, PasswordCheck::Rejected);
}

#[tokio::test]
async fn a_hash_below_the_current_cost_is_rewritten() {
    let password = Password::new("correct horse battery");
    let old = cheap().hash(&password).await.expect("a cheap hash");

    let check = dearer().check(&password, &old).await.expect("a check");

    match check {
        PasswordCheck::AcceptedBehindCost(upgraded) => {
            assert_ne!(upgraded.encoded(), old.encoded());
        }
        other => panic!("the stale cost went unnoticed: {other:?}"),
    }
}

#[tokio::test]
async fn a_session_resolves_to_its_account() {
    let Some(pool) = control_plane("auth_session").await else {
        return;
    };
    let account = an_account(&pool, "person@example.com", Some("secret")).await;
    let sessions = PostgresSessions::new(pool.clone(), FixedClock::at(NOW), lifetime());

    let token = sessions.issue(account, None).await.expect("a session");
    let session = sessions.resolve(&token).await.expect("it resolves");

    assert!(token.looks_like_one());
    assert_eq!(session.account, account);
    assert_eq!(session.org, None);
}

#[tokio::test]
async fn a_revoked_session_resolves_no_longer() {
    let Some(pool) = control_plane("auth_revoked").await else {
        return;
    };
    let account = an_account(&pool, "person@example.com", Some("secret")).await;
    let sessions = PostgresSessions::new(pool.clone(), FixedClock::at(NOW), lifetime());
    let token = sessions.issue(account, None).await.expect("a session");

    sessions.revoke(&token).await.expect("it revokes");

    assert_eq!(sessions.resolve(&token).await, Err(SessionError::Unknown));
}

#[tokio::test]
async fn an_expired_session_resolves_no_longer() {
    let Some(pool) = control_plane("auth_expired").await else {
        return;
    };
    let account = an_account(&pool, "person@example.com", Some("secret")).await;
    let early = PostgresSessions::new(pool.clone(), FixedClock::at(NOW), lifetime());
    let late = PostgresSessions::new(pool.clone(), FixedClock::at(NOW + 101), lifetime());
    let token = early.issue(account, None).await.expect("a session");

    assert_eq!(late.resolve(&token).await, Err(SessionError::Unknown));
}

#[tokio::test]
async fn resolving_slides_the_expiry_but_not_past_the_hard_life() {
    let Some(pool) = control_plane("auth_sliding").await else {
        return;
    };
    let account = an_account(&pool, "person@example.com", Some("secret")).await;
    let early = PostgresSessions::new(pool.clone(), FixedClock::at(NOW), lifetime());
    let token = early.issue(account, None).await.expect("a session");

    let mut expiry = 0;
    for step in (50..=950).step_by(90) {
        let store = PostgresSessions::new(pool.clone(), FixedClock::at(NOW + step), lifetime());
        let session = store
            .resolve(&token)
            .await
            .expect("a session kept in use stays alive");
        expiry = session.expires_at.get();
        if step == 50 {
            assert_eq!(expiry, NOW + 150, "the first sighting slides one window");
        }
    }

    assert_eq!(
        expiry,
        NOW + 1000,
        "the hard life caps the window however often it is used"
    );
}

#[tokio::test]
async fn pruning_removes_only_what_has_expired() {
    let Some(pool) = control_plane("auth_prune").await else {
        return;
    };
    let account = an_account(&pool, "person@example.com", Some("secret")).await;
    let early = PostgresSessions::new(pool.clone(), FixedClock::at(NOW), lifetime());
    early.issue(account, None).await.expect("a session");
    let later = PostgresSessions::new(pool.clone(), FixedClock::at(NOW + 500), lifetime());
    later.issue(account, None).await.expect("a second session");

    let removed = PostgresSessions::new(pool.clone(), FixedClock::at(NOW + 101), lifetime())
        .prune()
        .await
        .expect("a prune");

    let left: i64 = sqlx::query("SELECT count(*) AS n FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("counting sessions")
        .get("n");
    assert_eq!((removed, left), (1, 1));
}

#[tokio::test]
async fn a_correct_password_resolves_a_local_identity() {
    let Some(pool) = control_plane("auth_local_ok").await else {
        return;
    };
    let account = an_account(&pool, "person@example.com", Some("correct horse")).await;
    let provider = LocalPasswordProvider::new(pool.clone(), cheap());

    let identity = provider
        .sign_in(a_proof("person@example.com", "correct horse"))
        .await
        .expect("the password matches");

    assert!(identity.issuer.is_local());
    assert_eq!(identity.subject.as_str(), account.as_uuid().to_string());
    assert_eq!(identity.email.as_str(), "person@example.com");
}

#[tokio::test]
async fn a_wrong_password_an_unknown_address_and_no_password_all_refuse() {
    let Some(pool) = control_plane("auth_local_refused").await else {
        return;
    };
    an_account(&pool, "person@example.com", Some("correct horse")).await;
    an_account(&pool, "federated@example.com", None).await;
    let provider = LocalPasswordProvider::new(pool.clone(), cheap());

    for proof in [
        a_proof("person@example.com", "wrong horse"),
        a_proof("stranger@example.com", "correct horse"),
        a_proof("federated@example.com", "correct horse"),
    ] {
        assert_eq!(provider.sign_in(proof).await, Err(AuthError::Rejected));
    }
}
