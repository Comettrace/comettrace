use comettrace_core::{
    DecompressedBytes, Email, Issuer, ProvisionPolicy, ResolvedIdentity, Subject,
};
use comettrace_storage::{
    AccountError, Allowance, CONTROL_PLANE, OVERAGE_FEATURE, PostgresAccounts, SELF_HOSTED_PLAN,
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
