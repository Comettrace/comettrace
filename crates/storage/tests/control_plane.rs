use comettrace_core::{
    DecompressedBytes, Email, IngestKey, Issuer, ProvisionPolicy, ResolvedIdentity, SecretHash,
    Subject, TenantDirectory, TenantId, UnknownKey,
};
use comettrace_storage::{
    AccountError, Allowance, CONTROL_PLANE, FixedClock, OVERAGE_FEATURE, PostgresAccounts,
    PostgresDirectory, SELF_HOSTED_PLAN,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool, Row};

fn url() -> Option<String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Some(url);
    }
    assert!(
        std::env::var("REQUIRE_DATABASE_URL").is_err(),
        "REQUIRE_DATABASE_URL is set but DATABASE_URL is not"
    );
    None
}

async fn migrated(schema: &str) -> Option<PgPool> {
    let url = url()?;
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

#[tokio::test]
async fn the_self_hosted_plan_leaves_every_limit_open() {
    let Some(pool) = migrated("control_plan").await else {
        return;
    };

    let row = sqlx::query(
        "SELECT ingest_gb, retention_days, max_members, features \
         FROM plans WHERE slug = $1",
    )
    .bind(SELF_HOSTED_PLAN)
    .fetch_one(&pool)
    .await
    .expect("the default plan exists");

    assert_eq!(row.get::<Option<i32>, _>("ingest_gb"), None);
    assert_eq!(row.get::<Option<i32>, _>("retention_days"), None);
    assert_eq!(row.get::<Option<i32>, _>("max_members"), None);
    assert_eq!(row.get::<Vec<String>, _>("features"), Vec::<String>::new());
}

#[tokio::test]
async fn an_account_may_hold_only_a_password() {
    let Some(pool) = migrated("control_password_only").await else {
        return;
    };

    sqlx::query(
        "INSERT INTO accounts (id, email, password_hash) \
         VALUES (gen_random_uuid(), 'local@example.com', '$argon2id$hash')",
    )
    .execute(&pool)
    .await
    .expect("a password is a way in on its own");
}

#[tokio::test]
async fn an_account_may_now_exist_before_any_way_in_is_linked() {
    let Some(pool) = migrated("control_no_way_in").await else {
        return;
    };

    sqlx::query(
        "INSERT INTO accounts (id, email, password_hash) \
         VALUES (gen_random_uuid(), 'nobody@example.com', NULL)",
    )
    .execute(&pool)
    .await
    .expect("the cross table constraint is gone, so this row is accepted");

    let linked: i64 = sqlx::query("SELECT count(*) AS n FROM identity_links")
        .fetch_one(&pool)
        .await
        .expect("counting links")
        .get("n");
    assert_eq!(linked, 0, "and it can authenticate through nothing");
}

#[tokio::test]
async fn an_api_key_prefix_is_claimed_once() {
    let Some(pool) = migrated("control_prefix").await else {
        return;
    };
    let org = seeded_org(&pool).await;

    sqlx::query(
        "INSERT INTO api_keys (id, org_id, prefix, hash) \
         VALUES (gen_random_uuid(), $1, 'ct_live_abc123', '$argon2id$first')",
    )
    .bind(org)
    .execute(&pool)
    .await
    .expect("the first key takes the prefix");

    let refused = sqlx::query(
        "INSERT INTO api_keys (id, org_id, prefix, hash) \
         VALUES (gen_random_uuid(), $1, 'ct_live_abc123', '$argon2id$second')",
    )
    .bind(org)
    .execute(&pool)
    .await;

    assert!(
        refused.is_err(),
        "two keys shared one prefix, so a lookup by prefix is no longer a single row"
    );
}

#[tokio::test]
async fn an_entitlement_refuses_zero_but_accepts_nothing_at_all() {
    let Some(pool) = migrated("control_entitlement").await else {
        return;
    };

    let refused = sqlx::query(
        "INSERT INTO plans (id, slug, name, ingest_gb) \
         VALUES (gen_random_uuid(), 'broken', 'Broken', 0)",
    )
    .execute(&pool)
    .await;
    let accepted = sqlx::query(
        "INSERT INTO plans (id, slug, name, ingest_gb) \
         VALUES (gen_random_uuid(), 'unlimited', 'Unlimited', NULL)",
    )
    .execute(&pool)
    .await;

    assert!(refused.is_err(), "a zero allowance is not a limit");
    assert!(accepted.is_ok(), "NULL must stand for unlimited");
}

#[tokio::test]
async fn dropping_an_org_takes_its_projects_and_keys() {
    let Some(pool) = migrated("control_cascade").await else {
        return;
    };
    let org = seeded_org(&pool).await;
    sqlx::query("INSERT INTO projects (id, org_id, name) VALUES (gen_random_uuid(), $1, 'web')")
        .bind(org)
        .execute(&pool)
        .await
        .expect("a project");
    sqlx::query(
        "INSERT INTO api_keys (id, org_id, prefix, hash) \
         VALUES (gen_random_uuid(), $1, 'ct_live_xyz', '$argon2id$hash')",
    )
    .bind(org)
    .execute(&pool)
    .await
    .expect("a key");

    sqlx::query("DELETE FROM orgs WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await
        .expect("the org goes");

    for table in ["projects", "api_keys"] {
        let left: i64 = sqlx::query(AssertSqlSafe(format!("SELECT count(*) AS n FROM {table}")))
            .fetch_one(&pool)
            .await
            .expect("counting")
            .get("n");
        assert_eq!(left, 0, "{table} kept rows for a deleted org");
    }
}

#[tokio::test]
async fn the_migrations_reverse_and_reapply() {
    let Some(pool) = migrated("control_cycle").await else {
        return;
    };

    CONTROL_PLANE
        .undo(&pool, 0)
        .await
        .expect("the migrations revert");
    let gone: i64 = sqlx::query(
        "SELECT count(*) AS n FROM information_schema.tables \
         WHERE table_schema = 'control_cycle' AND table_name = 'orgs'",
    )
    .fetch_one(&pool)
    .await
    .expect("counting the tables")
    .get("n");
    CONTROL_PLANE
        .run(&pool)
        .await
        .expect("the migrations apply again");

    assert_eq!(gone, 0, "the down step left the orgs table behind");
    assert!(
        sqlx::query("SELECT slug FROM plans WHERE slug = $1")
            .bind(SELF_HOSTED_PLAN)
            .fetch_optional(&pool)
            .await
            .expect("looking for the default plan")
            .is_some(),
        "reapplying the migrations did not restore the default plan"
    );
}

async fn seeded_org(pool: &PgPool) -> uuid::Uuid {
    let plan: uuid::Uuid = sqlx::query("SELECT id AS id FROM plans WHERE slug = $1")
        .bind(SELF_HOSTED_PLAN)
        .fetch_one(pool)
        .await
        .expect("the default plan")
        .get("id");
    let org = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO orgs (id, name, plan_id) VALUES ($1, 'Acme', $2)")
        .bind(org)
        .bind(plan)
        .execute(pool)
        .await
        .expect("an org");
    org
}

async fn plan_with(
    pool: &PgPool,
    slug: &str,
    ingest_gb: Option<i32>,
    features: &[&str],
) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO plans (id, slug, name, ingest_gb, features) VALUES ($1, $2, $2, $3, $4)",
    )
    .bind(id)
    .bind(slug)
    .bind(ingest_gb)
    .bind(features.iter().map(|f| (*f).to_owned()).collect::<Vec<_>>())
    .execute(pool)
    .await
    .expect("a plan");
    id
}

async fn org_on(pool: &PgPool, plan: uuid::Uuid) -> comettrace_core::TenantId {
    let org = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO orgs (id, name, plan_id) VALUES ($1, 'Acme', $2)")
        .bind(org)
        .bind(plan)
        .execute(pool)
        .await
        .expect("an org");
    comettrace_core::TenantId::new(org)
}

#[tokio::test]
async fn a_plan_without_an_ingest_limit_loads_as_unlimited() {
    let Some(pool) = migrated("control_unlimited").await else {
        return;
    };
    let plan = plan_with(&pool, "generous", None, &[]).await;
    let tenant = org_on(&pool, plan).await;

    let allowances = comettrace_storage::load_allowances(&pool)
        .await
        .expect("the allowances load");

    assert_eq!(allowances.get(&tenant), Some(&Allowance::Unlimited));
}

#[tokio::test]
async fn a_plan_without_the_overage_feature_caps_hard() {
    let Some(pool) = migrated("control_capped").await else {
        return;
    };
    let plan = plan_with(&pool, "free", Some(2), &[]).await;
    let tenant = org_on(&pool, plan).await;

    let allowances = comettrace_storage::load_allowances(&pool)
        .await
        .expect("the allowances load");

    assert_eq!(
        allowances.get(&tenant),
        Some(&Allowance::Capped(DecompressedBytes::new(
            2 * 1024 * 1024 * 1024
        )))
    );
}

#[tokio::test]
async fn a_plan_carrying_the_overage_feature_allows_the_overage() {
    let Some(pool) = migrated("control_overage").await else {
        return;
    };
    let plan = plan_with(&pool, "pro", Some(1), &[OVERAGE_FEATURE, "sso"]).await;
    let tenant = org_on(&pool, plan).await;

    let allowances = comettrace_storage::load_allowances(&pool)
        .await
        .expect("the allowances load");

    assert_eq!(
        allowances.get(&tenant),
        Some(&Allowance::Overage(DecompressedBytes::new(
            1024 * 1024 * 1024
        )))
    );
}

#[tokio::test]
async fn an_org_on_the_default_plan_is_unlimited() {
    let Some(pool) = migrated("control_default_plan").await else {
        return;
    };
    let tenant = comettrace_core::TenantId::new(seeded_org(&pool).await);

    let allowances = comettrace_storage::load_allowances(&pool)
        .await
        .expect("the allowances load");

    assert_eq!(allowances.get(&tenant), Some(&Allowance::Unlimited));
}

fn an_identity(issuer: Issuer, subject: &str, email: &str) -> ResolvedIdentity {
    ResolvedIdentity {
        issuer,
        subject: Subject::try_from(subject).expect("a subject"),
        email: Email::try_from(email).expect("an address"),
        email_verified: true,
    }
}

async fn account_count(pool: &PgPool) -> i64 {
    sqlx::query("SELECT count(*) AS n FROM accounts")
        .fetch_one(pool)
        .await
        .expect("counting accounts")
        .get("n")
}

#[tokio::test]
async fn provisioning_creates_an_account_and_its_link_together() {
    let Some(pool) = migrated("identity_provision").await else {
        return;
    };
    let accounts = PostgresAccounts::new(pool.clone());
    let identity = an_identity(Issuer::Local, "sub-1", "person@example.com");

    let account = accounts
        .provision(&identity, ProvisionPolicy::default())
        .await
        .expect("the identity is provisioned");

    assert_eq!(account_count(&pool).await, 1);
    assert_eq!(
        accounts.linked(&identity).await.expect("a lookup"),
        Some(account)
    );
}

#[tokio::test]
async fn provisioning_the_same_identity_twice_returns_the_same_account() {
    let Some(pool) = migrated("identity_twice").await else {
        return;
    };
    let accounts = PostgresAccounts::new(pool.clone());
    let identity = an_identity(Issuer::Local, "sub-1", "person@example.com");

    let first = accounts
        .provision(&identity, ProvisionPolicy::default())
        .await
        .expect("the first provision");
    let second = accounts
        .provision(&identity, ProvisionPolicy::default())
        .await
        .expect("the second provision");

    assert_eq!(first, second);
    assert_eq!(account_count(&pool).await, 1);
}

#[tokio::test]
async fn two_providers_may_carry_the_same_subject() {
    let Some(pool) = migrated("identity_two_providers").await else {
        return;
    };
    let accounts = PostgresAccounts::new(pool.clone());
    let local = an_identity(Issuer::Local, "shared", "local@example.com");
    let federated = an_identity(
        Issuer::federated("https://id.example").expect("a url"),
        "shared",
        "federated@example.com",
    );

    let first = accounts
        .provision(&local, ProvisionPolicy::default())
        .await
        .expect("the local identity");
    let second = accounts
        .provision(&federated, ProvisionPolicy::default())
        .await
        .expect("the federated identity");

    assert_ne!(
        first, second,
        "one subject under two issuers is two identities"
    );
    assert_eq!(account_count(&pool).await, 2);
}

#[tokio::test]
async fn linking_by_email_joins_an_existing_account() {
    let Some(pool) = migrated("identity_by_email").await else {
        return;
    };
    let accounts = PostgresAccounts::new(pool.clone());
    let local = an_identity(Issuer::Local, "sub-1", "person@example.com");
    let federated = an_identity(
        Issuer::federated("https://id.example").expect("a url"),
        "sub-2",
        "person@example.com",
    );

    let first = accounts
        .provision(&local, ProvisionPolicy::default())
        .await
        .expect("the local identity");
    let second = accounts
        .provision(
            &federated,
            ProvisionPolicy {
                auto_provision: false,
                link_by_email: true,
            },
        )
        .await
        .expect("the federated identity joins by email");

    assert_eq!(first, second);
    assert_eq!(account_count(&pool).await, 1);
}

#[tokio::test]
async fn an_unknown_identity_is_refused_when_provisioning_is_off() {
    let Some(pool) = migrated("identity_closed").await else {
        return;
    };
    let accounts = PostgresAccounts::new(pool.clone());
    let identity = an_identity(Issuer::Local, "sub-1", "stranger@example.com");

    let refused = accounts
        .provision(
            &identity,
            ProvisionPolicy {
                auto_provision: false,
                link_by_email: false,
            },
        )
        .await;

    assert!(matches!(refused, Err(AccountError::NotProvisioned)));
    assert_eq!(account_count(&pool).await, 0, "nothing was left behind");
}

async fn a_key(pool: &PgPool, org: uuid::Uuid, prefix: &str, secret: Option<&str>) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    let digest = secret.map(|plain| SecretHash::of(plain).as_bytes().to_vec());
    sqlx::query(
        "INSERT INTO api_keys (id, org_id, prefix, hash, secret_digest) \
         VALUES ($1, $2, $3, 'superseded', $4)",
    )
    .bind(id)
    .bind(org)
    .bind(prefix)
    .bind(digest)
    .execute(pool)
    .await
    .expect("a key");
    id
}

async fn last_seen(pool: &PgPool, key: uuid::Uuid) -> Option<i64> {
    sqlx::query("SELECT last_used_at AS last_used_at FROM api_keys WHERE id = $1")
        .bind(key)
        .fetch_one(pool)
        .await
        .expect("reading the sighting")
        .get("last_used_at")
}

#[tokio::test]
async fn a_live_key_resolves_to_its_organisation() {
    let Some(pool) = migrated("keys_live").await else {
        return;
    };
    let org = seeded_org(&pool).await;
    a_key(&pool, org, "ct_live_abc", Some("the-secret")).await;
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    let resolved = directory
        .resolve(&IngestKey::from_plaintext("ct_live_abc.the-secret"))
        .await;

    assert_eq!(resolved, Ok(TenantId::new(org)));
}

#[tokio::test]
async fn a_wrong_secret_an_unknown_prefix_and_a_malformed_key_all_refuse() {
    let Some(pool) = migrated("keys_refused").await else {
        return;
    };
    let org = seeded_org(&pool).await;
    a_key(&pool, org, "ct_live_abc", Some("the-secret")).await;
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    for candidate in [
        "ct_live_abc.wrong-secret",
        "ct_live_zzz.the-secret",
        "ct_live_abc",
    ] {
        assert_eq!(
            directory
                .resolve(&IngestKey::from_plaintext(candidate))
                .await,
            Err(UnknownKey),
            "{candidate} should be refused"
        );
    }
}

#[tokio::test]
async fn a_revoked_key_no_longer_resolves() {
    let Some(pool) = migrated("keys_revoked").await else {
        return;
    };
    let org = seeded_org(&pool).await;
    let key = a_key(&pool, org, "ct_live_abc", Some("the-secret")).await;
    sqlx::query("UPDATE api_keys SET revoked_at = 1 WHERE id = $1")
        .bind(key)
        .execute(&pool)
        .await
        .expect("the revocation");
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    assert_eq!(
        directory
            .resolve(&IngestKey::from_plaintext("ct_live_abc.the-secret"))
            .await,
        Err(UnknownKey)
    );
}

#[tokio::test]
async fn a_key_left_without_a_digest_cannot_authenticate() {
    let Some(pool) = migrated("keys_no_digest").await else {
        return;
    };
    let org = seeded_org(&pool).await;
    a_key(&pool, org, "ct_live_abc", None).await;
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    assert_eq!(
        directory
            .resolve(&IngestKey::from_plaintext("ct_live_abc.the-secret"))
            .await,
        Err(UnknownKey)
    );
}

#[tokio::test]
async fn a_sighting_is_recorded_once_then_left_alone_for_a_while() {
    let Some(pool) = migrated("keys_sighting").await else {
        return;
    };
    let org = seeded_org(&pool).await;
    let key = a_key(&pool, org, "ct_live_abc", Some("the-secret")).await;
    let ingest = IngestKey::from_plaintext("ct_live_abc.the-secret");
    let now = 1_700_000_000;

    assert_eq!(last_seen(&pool, key).await, None);
    PostgresDirectory::new(pool.clone(), FixedClock::at(now))
        .resolve(&ingest)
        .await
        .expect("the first use");
    let first = last_seen(&pool, key).await;

    PostgresDirectory::new(pool.clone(), FixedClock::at(now + 60))
        .resolve(&ingest)
        .await
        .expect("a use inside the gap");
    let unchanged = last_seen(&pool, key).await;

    PostgresDirectory::new(pool.clone(), FixedClock::at(now + 400))
        .resolve(&ingest)
        .await
        .expect("a use past the gap");
    let moved = last_seen(&pool, key).await;

    assert!(first.is_some());
    assert_eq!(unchanged, first, "a sighting inside the gap costs no write");
    assert_ne!(moved, first, "a sighting past the gap is recorded");
}

#[tokio::test]
async fn an_issued_key_authenticates_at_once() {
    let Some(pool) = migrated("keys_issued").await else {
        return;
    };
    let org = TenantId::new(seeded_org(&pool).await);
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    let issued = directory.issue(org, None).await.expect("a key is issued");
    let resolved = directory.resolve(&issued).await;

    assert_eq!(resolved, Ok(org));
    assert_eq!(directory.live_keys(org).await.expect("a count"), 1);
}

#[tokio::test]
async fn an_issued_key_carries_a_prefix_and_a_long_secret() {
    let Some(pool) = migrated("keys_shape").await else {
        return;
    };
    let org = TenantId::new(seeded_org(&pool).await);
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    let issued = directory.issue(org, None).await.expect("a key is issued");
    let (prefix, secret) = issued.split().expect("it splits");

    assert!(prefix.starts_with("ct_live_"), "{prefix}");
    assert_eq!(secret.len(), 64, "a 32 byte secret in hex");
    assert_eq!(format!("{issued:?}"), "IngestKey(redacted)");
}

#[tokio::test]
async fn two_issued_keys_never_collide() {
    let Some(pool) = migrated("keys_distinct").await else {
        return;
    };
    let org = TenantId::new(seeded_org(&pool).await);
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    let first = directory.issue(org, None).await.expect("the first");
    let second = directory.issue(org, None).await.expect("the second");

    assert_ne!(first.exposed(), second.exposed());
    assert_eq!(directory.resolve(&first).await, Ok(org));
    assert_eq!(directory.resolve(&second).await, Ok(org));
    assert_eq!(directory.live_keys(org).await.expect("a count"), 2);
}

#[tokio::test]
async fn revoking_a_key_stops_it_without_touching_the_others() {
    let Some(pool) = migrated("keys_revoke").await else {
        return;
    };
    let org = TenantId::new(seeded_org(&pool).await);
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));
    let doomed = directory.issue(org, None).await.expect("the first");
    let kept = directory.issue(org, None).await.expect("the second");
    let (prefix, _) = doomed.split().expect("it splits");

    let revoked = directory.revoke(prefix).await.expect("a revocation");

    assert!(revoked);
    assert_eq!(directory.resolve(&doomed).await, Err(UnknownKey));
    assert_eq!(directory.resolve(&kept).await, Ok(org));
    assert_eq!(directory.live_keys(org).await.expect("a count"), 1);
}

#[tokio::test]
async fn revoking_a_prefix_nobody_issued_changes_nothing() {
    let Some(pool) = migrated("keys_revoke_absent").await else {
        return;
    };
    let directory = PostgresDirectory::new(pool.clone(), FixedClock::at(1_700_000_000));

    assert!(!directory.revoke("ct_live_nothing").await.expect("a call"));
}
