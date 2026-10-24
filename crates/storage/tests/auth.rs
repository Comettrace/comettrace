use comettrace_core::{
    AccountId, AuthError, AuthProvider, CodeVerifier, Email, FlowError, Mailer, OidcFlows,
    Password, PasswordCheck, PasswordHasher, PendingFlow, ResetError, ResetPassword, ResetStore,
    ResetToken, SessionError, SessionLifetime, SessionStore, SignInProof,
};
use comettrace_storage::{
    AccountError, Argon2Hasher, CONTROL_PLANE, FixedClock, LocalPasswordProvider, PostgresAccounts,
    PostgresFlows, PostgresResets, PostgresSessions,
};
use std::sync::Mutex;
use std::time::Duration;

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

fn a_flow(state: &str) -> PendingFlow {
    PendingFlow {
        state: state.to_owned(),
        nonce: "the-nonce".to_owned(),
        verifier: CodeVerifier::from_plaintext("the-verifier"),
        redirect_uri: "https://app.example.com/callback".to_owned(),
    }
}

#[tokio::test]
async fn an_attempt_comes_back_once_and_only_once() {
    let Some(pool) = control_plane("flows_once").await else {
        return;
    };
    let flows = PostgresFlows::new(pool.clone(), FixedClock::at(NOW));
    flows.begin(&a_flow("state-1")).await.expect("it is kept");

    let first = flows.take("state-1").await.expect("it comes back");
    let second = flows.take("state-1").await;

    assert_eq!(first.nonce, "the-nonce");
    assert_eq!(first.verifier.exposed(), "the-verifier");
    assert_eq!(first.redirect_uri, "https://app.example.com/callback");
    assert_eq!(
        second,
        Err(FlowError::Unknown),
        "a replay must find nothing"
    );
}

#[tokio::test]
async fn an_unknown_state_carries_no_attempt() {
    let Some(pool) = control_plane("flows_unknown").await else {
        return;
    };
    let flows = PostgresFlows::new(pool.clone(), FixedClock::at(NOW));

    assert_eq!(flows.take("never-issued").await, Err(FlowError::Unknown));
}

#[tokio::test]
async fn an_attempt_past_its_window_is_gone() {
    let Some(pool) = control_plane("flows_expired").await else {
        return;
    };
    let early =
        PostgresFlows::new(pool.clone(), FixedClock::at(NOW)).with_window(Duration::from_secs(60));
    let late = PostgresFlows::new(pool.clone(), FixedClock::at(NOW + 61))
        .with_window(Duration::from_secs(60));
    early.begin(&a_flow("state-1")).await.expect("it is kept");

    assert_eq!(late.take("state-1").await, Err(FlowError::Unknown));
}

#[tokio::test]
async fn two_attempts_do_not_shadow_each_other() {
    let Some(pool) = control_plane("flows_two").await else {
        return;
    };
    let flows = PostgresFlows::new(pool.clone(), FixedClock::at(NOW));
    flows.begin(&a_flow("state-1")).await.expect("the first");
    let mut second = a_flow("state-2");
    second.nonce = "another-nonce".to_owned();
    flows.begin(&second).await.expect("the second");

    let taken = flows.take("state-2").await.expect("the second comes back");

    assert_eq!(taken.nonce, "another-nonce");
    assert!(
        flows.take("state-1").await.is_ok(),
        "the first is untouched"
    );
}

#[tokio::test]
async fn pruning_removes_only_attempts_past_their_window() {
    let Some(pool) = control_plane("flows_prune").await else {
        return;
    };
    let early =
        PostgresFlows::new(pool.clone(), FixedClock::at(NOW)).with_window(Duration::from_secs(60));
    early.begin(&a_flow("state-1")).await.expect("the first");
    let later = PostgresFlows::new(pool.clone(), FixedClock::at(NOW + 500))
        .with_window(Duration::from_secs(60));
    later.begin(&a_flow("state-2")).await.expect("the second");

    let removed = PostgresFlows::new(pool.clone(), FixedClock::at(NOW + 61))
        .prune()
        .await
        .expect("a prune");

    assert_eq!(removed, 1);
    assert!(later.take("state-2").await.is_ok(), "the live one stayed");
}

#[derive(Default)]
struct Outbox {
    sent: Mutex<Vec<String>>,
}

impl Outbox {
    fn last(&self) -> Option<String> {
        self.sent.lock().expect("the outbox lock").last().cloned()
    }

    fn count(&self) -> usize {
        self.sent.lock().expect("the outbox lock").len()
    }
}

impl Mailer for &Outbox {
    async fn send_password_reset(&self, _to: &Email, token: &ResetToken) -> Result<(), ResetError> {
        self.sent
            .lock()
            .expect("the outbox lock")
            .push(token.exposed().to_owned());
        Ok(())
    }
}

fn a_reset_token(seed: &str) -> ResetToken {
    ResetToken::from_plaintext(&format!("ct_pr_{seed}"))
}

#[tokio::test]
async fn a_reset_is_offered_only_to_an_address_we_know() {
    let Some(pool) = control_plane("reset_known").await else {
        return;
    };
    an_account(&pool, "person@example.com", Some("old secret")).await;
    let outbox = Outbox::default();
    let reset = ResetPassword::new(
        PostgresResets::new(pool.clone(), FixedClock::at(NOW)),
        cheap(),
        &outbox,
    );

    let known = reset
        .request(
            &Email::try_from("person@example.com").expect("an address"),
            a_reset_token("one"),
        )
        .await
        .expect("the request runs");
    let unknown = reset
        .request(
            &Email::try_from("stranger@example.com").expect("an address"),
            a_reset_token("two"),
        )
        .await
        .expect("the request runs");

    assert!(known);
    assert!(!unknown);
    assert_eq!(
        outbox.count(),
        1,
        "no message leaves for an unknown address"
    );
}

#[tokio::test]
async fn a_confirmed_reset_changes_the_password_and_ends_every_session() {
    let Some(pool) = control_plane("reset_confirm").await else {
        return;
    };
    let account = an_account(&pool, "person@example.com", Some("old secret")).await;
    let sessions = PostgresSessions::new(pool.clone(), FixedClock::at(NOW), lifetime());
    let token = sessions.issue(account, None).await.expect("a session");
    let outbox = Outbox::default();
    let reset = ResetPassword::new(
        PostgresResets::new(pool.clone(), FixedClock::at(NOW)),
        cheap(),
        &outbox,
    );
    let address = Email::try_from("person@example.com").expect("an address");
    reset
        .request(&address, a_reset_token("one"))
        .await
        .expect("the request");
    let sent = ResetToken::from_plaintext(&outbox.last().expect("a message"));

    let ended = reset
        .confirm(&sent, &Password::new("a new secret"))
        .await
        .expect("the confirmation");

    assert_eq!(ended, 1, "the session it was signed in with is gone");
    assert_eq!(sessions.resolve(&token).await, Err(SessionError::Unknown));

    let provider = LocalPasswordProvider::new(pool.clone(), cheap());
    assert!(
        provider
            .sign_in(a_proof("person@example.com", "a new secret"))
            .await
            .is_ok(),
        "the new password works"
    );
    assert_eq!(
        provider
            .sign_in(a_proof("person@example.com", "old secret"))
            .await,
        Err(AuthError::Rejected),
        "the old one does not"
    );
}

#[tokio::test]
async fn a_reset_token_is_spent_by_its_first_use() {
    let Some(pool) = control_plane("reset_once").await else {
        return;
    };
    an_account(&pool, "person@example.com", Some("old secret")).await;
    let outbox = Outbox::default();
    let reset = ResetPassword::new(
        PostgresResets::new(pool.clone(), FixedClock::at(NOW)),
        cheap(),
        &outbox,
    );
    let token = a_reset_token("one");
    reset
        .request(
            &Email::try_from("person@example.com").expect("an address"),
            token.clone(),
        )
        .await
        .expect("the request");

    reset
        .confirm(&token, &Password::new("a new secret"))
        .await
        .expect("the first use");
    let replay = reset.confirm(&token, &Password::new("another")).await;

    assert_eq!(replay, Err(ResetError::Unknown));
}

#[tokio::test]
async fn a_reset_past_its_window_is_gone() {
    let Some(pool) = control_plane("reset_expired").await else {
        return;
    };
    an_account(&pool, "person@example.com", Some("old secret")).await;
    let outbox = Outbox::default();
    let token = a_reset_token("one");
    ResetPassword::new(
        PostgresResets::new(pool.clone(), FixedClock::at(NOW)).with_window(Duration::from_secs(60)),
        cheap(),
        &outbox,
    )
    .request(
        &Email::try_from("person@example.com").expect("an address"),
        token.clone(),
    )
    .await
    .expect("the request");

    let late = ResetPassword::new(
        PostgresResets::new(pool.clone(), FixedClock::at(NOW + 61))
            .with_window(Duration::from_secs(60)),
        cheap(),
        &outbox,
    )
    .confirm(&token, &Password::new("a new secret"))
    .await;

    assert_eq!(late, Err(ResetError::Unknown));
}

#[tokio::test]
async fn a_created_account_is_locked_until_a_password_is_chosen() {
    let Some(pool) = control_plane("accounts_locked").await else {
        return;
    };
    let address = Email::try_from("fresh@example.com").expect("an address");
    let locked = cheap().locked().expect("a locked hash");
    let account = PostgresAccounts::new(pool.clone())
        .create(&address, &locked)
        .await
        .expect("the account is created");
    let provider = LocalPasswordProvider::new(pool.clone(), cheap());

    assert_eq!(
        provider.sign_in(a_proof("fresh@example.com", "")).await,
        Err(AuthError::Rejected),
        "an empty password must not open a locked account"
    );
    assert_eq!(
        provider
            .sign_in(a_proof("fresh@example.com", "correct horse"))
            .await,
        Err(AuthError::Rejected)
    );

    let outbox = Outbox::default();
    let resets = PostgresResets::new(pool.clone(), FixedClock::at(NOW));
    let invite = a_reset_token("invite");
    resets
        .begin(account, invite.hash())
        .await
        .expect("the invite is opened");
    ResetPassword::new(resets, cheap(), &outbox)
        .confirm(&invite, &Password::new("chosen at last"))
        .await
        .expect("the password is chosen");

    assert!(
        provider
            .sign_in(a_proof("fresh@example.com", "chosen at last"))
            .await
            .is_ok(),
        "the account opens once its password is chosen"
    );
}

#[tokio::test]
async fn an_address_carries_at_most_one_account() {
    let Some(pool) = control_plane("accounts_taken").await else {
        return;
    };
    let address = Email::try_from("twice@example.com").expect("an address");
    let accounts = PostgresAccounts::new(pool.clone());
    let locked = cheap().locked().expect("a locked hash");
    accounts
        .create(&address, &locked)
        .await
        .expect("the first account");

    let refused = accounts.create(&address, &locked).await;

    assert!(matches!(refused, Err(AccountError::Taken)));
}

#[tokio::test]
async fn two_locked_accounts_do_not_share_a_hash() {
    let hasher = cheap();
    let one = hasher.locked().expect("a locked hash");
    let other = hasher.locked().expect("another locked hash");

    assert_ne!(one.encoded(), other.encoded());
}
