use comettrace_core::{DecompressedBytes, TenantId, UsageMeter, UsageReader};
use comettrace_storage::{CONTROL_PLANE, FixedClock, RedisMeter, SELF_HOSTED_PLAN};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool, Row};
use uuid::Uuid;

const NOVEMBER: u64 = 1_700_000_000;
const DECEMBER: u64 = 1_702_600_000;

fn redis_url() -> Option<String> {
    if let Ok(url) = std::env::var("REDIS_URL") {
        return Some(url);
    }
    assert!(
        std::env::var("REQUIRE_REDIS_URL").is_err(),
        "REQUIRE_REDIS_URL is set but REDIS_URL is not"
    );
    None
}

async fn meter_at(seconds: u64) -> Option<RedisMeter<FixedClock>> {
    let url = redis_url()?;
    Some(
        RedisMeter::connect(&url, FixedClock::at(seconds))
            .await
            .expect("connecting to the usage counter store"),
    )
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

#[tokio::test]
async fn a_recorded_payload_shows_up_in_the_monthly_counter() {
    let Some(meter) = meter_at(NOVEMBER).await else {
        return;
    };
    let tenant = TenantId::new(Uuid::new_v4());

    meter
        .record_ingest(tenant, DecompressedBytes::new(400))
        .await;
    meter
        .record_ingest(tenant, DecompressedBytes::new(600))
        .await;

    assert_eq!(meter.used(tenant).await, DecompressedBytes::new(1000));
}

#[tokio::test]
async fn an_unseen_tenant_has_used_nothing() {
    let Some(meter) = meter_at(NOVEMBER).await else {
        return;
    };

    assert_eq!(
        meter.used(TenantId::new(Uuid::new_v4())).await,
        DecompressedBytes::new(0)
    );
}

#[tokio::test]
async fn two_tenants_do_not_share_a_counter() {
    let Some(meter) = meter_at(NOVEMBER).await else {
        return;
    };
    let first = TenantId::new(Uuid::new_v4());
    let second = TenantId::new(Uuid::new_v4());

    meter
        .record_ingest(first, DecompressedBytes::new(100))
        .await;

    assert_eq!(meter.used(first).await, DecompressedBytes::new(100));
    assert_eq!(meter.used(second).await, DecompressedBytes::new(0));
}

#[tokio::test]
async fn a_new_month_starts_the_counter_again() {
    let Some(november) = meter_at(NOVEMBER).await else {
        return;
    };
    let december = meter_at(DECEMBER).await.expect("the same store");
    let tenant = TenantId::new(Uuid::new_v4());

    november
        .record_ingest(tenant, DecompressedBytes::new(900))
        .await;

    assert_eq!(november.used(tenant).await, DecompressedBytes::new(900));
    assert_eq!(december.used(tenant).await, DecompressedBytes::new(0));
}

#[tokio::test]
async fn rehydration_seeds_the_counter_from_the_billed_hours() {
    let Some(pool) = control_plane("counter_rehydrate").await else {
        return;
    };
    let Some(meter) = meter_at(NOVEMBER).await else {
        return;
    };
    let tenant = seeded_org(&pool).await;
    billed(&pool, tenant, 1_698_800_000_000_000_000, 700).await;
    billed(&pool, tenant, 1_698_900_000_000_000_000, 300).await;

    let seeded = meter.rehydrate(&pool).await.expect("rehydration runs");

    assert_eq!(seeded, 1);
    assert_eq!(meter.used(tenant).await, DecompressedBytes::new(1000));
}

#[tokio::test]
async fn rehydration_leaves_a_live_counter_alone() {
    let Some(pool) = control_plane("counter_live").await else {
        return;
    };
    let Some(meter) = meter_at(NOVEMBER).await else {
        return;
    };
    let tenant = seeded_org(&pool).await;
    billed(&pool, tenant, 1_698_800_000_000_000_000, 700).await;
    meter
        .record_ingest(tenant, DecompressedBytes::new(50))
        .await;

    let seeded = meter.rehydrate(&pool).await.expect("rehydration runs");

    assert_eq!(seeded, 0, "a counter already in flight must not be reset");
    assert_eq!(meter.used(tenant).await, DecompressedBytes::new(50));
}

#[tokio::test]
async fn hours_before_the_month_are_not_rehydrated() {
    let Some(pool) = control_plane("counter_earlier").await else {
        return;
    };
    let Some(meter) = meter_at(NOVEMBER).await else {
        return;
    };
    let tenant = seeded_org(&pool).await;
    billed(&pool, tenant, 1_690_000_000_000_000_000, 5_000).await;

    meter.rehydrate(&pool).await.expect("rehydration runs");

    assert_eq!(meter.used(tenant).await, DecompressedBytes::new(0));
}

async fn billed(pool: &PgPool, tenant: TenantId, hour_start: i64, bytes: i64) {
    sqlx::query(
        "INSERT INTO usage_hourly (org_id, hour_start, ingested_bytes) VALUES ($1, $2, $3)",
    )
    .bind(tenant.as_uuid())
    .bind(hour_start)
    .bind(bytes)
    .execute(pool)
    .await
    .expect("a billed hour");
}

async fn seeded_org(pool: &PgPool) -> TenantId {
    let plan: Uuid = sqlx::query("SELECT id AS id FROM plans WHERE slug = $1")
        .bind(SELF_HOSTED_PLAN)
        .fetch_one(pool)
        .await
        .expect("the default plan")
        .get("id");
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs (id, name, plan_id) VALUES ($1, 'Acme', $2)")
        .bind(org)
        .bind(plan)
        .execute(pool)
        .await
        .expect("an org");
    TenantId::new(org)
}
