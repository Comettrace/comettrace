use std::collections::BTreeMap;
use std::time::Duration;

use clickhouse::Client;
use comettrace_core::{Span, SpanId, SpanKind, SpanSink, SpanStatus, TenantId, TraceId};
use comettrace_storage::{
    BatchLimits, ClickhouseMigrator, ClickhouseSink, FixedClock, MemorySink, SpanStoreSettings,
    open_span_store,
};
use uuid::Uuid;

const NOW: u64 = 4_000_000_000;

pub fn a_span(seed: u8, service: &str) -> Span {
    Span {
        trace_id: TraceId::try_from([seed; 16].as_slice()).expect("sixteen bytes"),
        span_id: SpanId::try_from([seed; 8].as_slice()).expect("eight bytes"),
        parent: None,
        name: "work".into(),
        service_name: service.into(),
        kind: SpanKind::Server,
        status: SpanStatus::Ok,
        start_unix_nanos: 1_700_000_000_000_000_000,
        end_unix_nanos: 1_700_000_000_500_000_000,
        attributes: BTreeMap::from([("http.status_code".into(), "200".into())]),
    }
}

async fn accepts_batches_from_several_tenants<S: SpanSink>(sink: &S) {
    let first = TenantId::new(Uuid::from_u128(1));
    let second = TenantId::new(Uuid::from_u128(2));

    sink.accept(first, vec![a_span(1, "checkout"), a_span(2, "checkout")])
        .await
        .expect("the first tenant's batch is accepted");
    sink.accept(second, vec![a_span(3, "payments")])
        .await
        .expect("the second tenant's batch is accepted");
}

async fn accepts_an_empty_batch<S: SpanSink>(sink: &S) {
    sink.accept(TenantId::new(Uuid::from_u128(9)), Vec::new())
        .await
        .expect("an empty batch is accepted");
}

#[tokio::test]
async fn the_memory_sink_honours_the_contract() {
    let sink = MemorySink::default();

    accepts_batches_from_several_tenants(&sink).await;
    accepts_an_empty_batch(&sink).await;

    assert_eq!(sink.accepted(), 3);
}

#[tokio::test]
async fn the_clickhouse_sink_honours_the_contract() {
    let Some(client) = prepared("comettrace_sink_contract").await else {
        return;
    };
    let first = TenantId::new(Uuid::from_u128(1));
    let second = TenantId::new(Uuid::from_u128(2));
    let sink = ClickhouseSink::new(
        &client,
        BatchLimits {
            max_rows: 1,
            period: Duration::from_secs(60),
        },
        Duration::from_secs(60 * 60),
        FixedClock::at(NOW),
    );

    sink.accept(first, vec![a_span(1, "checkout"), a_span(2, "checkout")])
        .await
        .expect("the first batch is accepted");
    assert_eq!(
        (sink.pending_rows().await, stored_rows(&client).await),
        (0, 2),
        "two rows written under a one row threshold must have been sent"
    );

    sink.accept(second, vec![a_span(3, "payments")])
        .await
        .expect("the second batch is accepted");
    assert_eq!(
        (sink.pending_rows().await, stored_rows(&client).await),
        (0, 3),
        "the row of the second tenant must have been sent too"
    );

    sink.accept(second, Vec::new())
        .await
        .expect("an empty batch is accepted");
    sink.flush().await.expect("the sink flushes");

    assert_eq!(
        stored_rows(&client).await,
        3,
        "an empty batch must add nothing"
    );
}

pub fn endpoint() -> Option<String> {
    if let Ok(url) = std::env::var("CLICKHOUSE_URL") {
        return Some(url);
    }
    assert!(
        std::env::var("REQUIRE_CLICKHOUSE_URL").is_err(),
        "REQUIRE_CLICKHOUSE_URL is set but CLICKHOUSE_URL is not"
    );
    None
}

pub fn connected(url: &str) -> Client {
    let mut client = Client::default().with_url(url);
    if let Ok(user) = std::env::var("CLICKHOUSE_USER") {
        client = client.with_user(user);
    }
    if let Ok(password) = std::env::var("CLICKHOUSE_PASSWORD") {
        client = client.with_password(password);
    }
    client
}

pub async fn prepared(name: &str) -> Option<Client> {
    let url = endpoint()?;
    let root = connected(&url);
    root.query(&format!("DROP DATABASE IF EXISTS {name}"))
        .execute()
        .await
        .expect("dropping a leftover database");
    root.query(&format!("CREATE DATABASE {name}"))
        .execute()
        .await
        .expect("creating the test database");
    let client = connected(&url).with_database(name);
    ClickhouseMigrator::new(client.clone())
        .up()
        .await
        .expect("the migrations apply");
    Some(client)
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct Counted {
    value: u64,
}

pub async fn stored_rows(client: &Client) -> u64 {
    client
        .query("SELECT count() AS value FROM spans")
        .fetch_one::<Counted>()
        .await
        .expect("counting the stored spans")
        .value
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct Expiry {
    value: u32,
}

async fn stored_expire_at(client: &Client) -> u32 {
    client
        .query("SELECT min(expire_at) AS value FROM spans")
        .fetch_one::<Expiry>()
        .await
        .expect("reading the expiry")
        .value
}

fn sink_with(client: &Client, limits: BatchLimits) -> ClickhouseSink<FixedClock> {
    ClickhouseSink::new(
        client,
        limits,
        Duration::from_secs(7 * 24 * 60 * 60),
        FixedClock::at(NOW),
    )
}

#[tokio::test]
async fn the_row_threshold_flushes_without_an_explicit_call() {
    let Some(client) = prepared("comettrace_sink_rows").await else {
        return;
    };
    let sink = sink_with(
        &client,
        BatchLimits {
            max_rows: 2,
            period: Duration::from_secs(60 * 60),
        },
    );

    sink.accept(
        TenantId::new(Uuid::from_u128(1)),
        vec![a_span(1, "checkout"), a_span(2, "checkout")],
    )
    .await
    .expect("the batch is accepted");

    assert_eq!(sink.pending_rows().await, 0, "the threshold did not flush");
    assert_eq!(stored_rows(&client).await, 2);
}

#[tokio::test]
async fn one_buffer_carries_rows_from_several_tenants() {
    let Some(client) = prepared("comettrace_sink_shared").await else {
        return;
    };
    let sink = sink_with(
        &client,
        BatchLimits {
            max_rows: 1_000,
            period: Duration::from_secs(60 * 60),
        },
    );

    sink.accept(
        TenantId::new(Uuid::from_u128(1)),
        vec![a_span(1, "checkout")],
    )
    .await
    .expect("the first tenant is accepted");
    sink.accept(
        TenantId::new(Uuid::from_u128(2)),
        vec![a_span(2, "payments")],
    )
    .await
    .expect("the second tenant is accepted");

    assert_eq!(
        sink.pending_rows().await,
        2,
        "the two tenants must share one buffer"
    );
    assert_eq!(stored_rows(&client).await, 0, "nothing should be sent yet");

    sink.flush().await.expect("the sink flushes");

    assert_eq!(stored_rows(&client).await, 2);
}

#[tokio::test(start_paused = true)]
async fn the_interval_flushes_once_the_period_has_elapsed() {
    let Some(client) = prepared("comettrace_sink_period").await else {
        return;
    };
    let sink = sink_with(
        &client,
        BatchLimits {
            max_rows: 1_000_000,
            period: Duration::from_secs(30),
        },
    );
    let tenant = TenantId::new(Uuid::from_u128(1));

    sink.accept(tenant, vec![a_span(1, "checkout")])
        .await
        .expect("the first batch is accepted");
    assert_eq!(sink.pending_rows().await, 1, "the row count did not flush");

    tokio::time::advance(Duration::from_secs(31)).await;
    sink.accept(tenant, vec![a_span(2, "checkout")])
        .await
        .expect("the second batch is accepted");

    assert_eq!(sink.pending_rows().await, 0, "the period did not flush");
    assert_eq!(stored_rows(&client).await, 2);
}

#[tokio::test]
async fn expire_at_is_the_clock_plus_the_retention() {
    let Some(client) = prepared("comettrace_sink_expiry").await else {
        return;
    };
    let sink = sink_with(
        &client,
        BatchLimits {
            max_rows: 1,
            period: Duration::from_secs(60 * 60),
        },
    );

    sink.accept(
        TenantId::new(Uuid::from_u128(1)),
        vec![a_span(1, "checkout")],
    )
    .await
    .expect("the batch is accepted");

    let expected = u32::try_from(NOW + 7 * 24 * 3600).expect("the expiry fits");
    assert_eq!(stored_expire_at(&client).await, expected);
}

#[tokio::test]
async fn a_timestamp_the_store_cannot_hold_is_refused() {
    let Some(client) = prepared("comettrace_sink_timestamp").await else {
        return;
    };
    let sink = sink_with(&client, BatchLimits::default());
    let mut span = a_span(1, "checkout");
    span.start_unix_nanos = u64::MAX;

    let outcome = sink
        .accept(TenantId::new(Uuid::from_u128(1)), vec![span])
        .await;

    assert_eq!(
        outcome,
        Err(comettrace_core::SinkError::TimestampOutOfRange)
    );
}

async fn an_empty_database(name: &str) -> Option<String> {
    let url = endpoint()?;
    let root = connected(&url);
    root.query(&format!("DROP DATABASE IF EXISTS {name}"))
        .execute()
        .await
        .expect("dropping a leftover database");
    root.query(&format!("CREATE DATABASE {name}"))
        .execute()
        .await
        .expect("creating the test database");
    Some(url)
}

fn settings_for(url: &str, database: &str) -> SpanStoreSettings {
    SpanStoreSettings {
        database: Some(database.to_owned()),
        user: std::env::var("CLICKHOUSE_USER").ok(),
        password: std::env::var("CLICKHOUSE_PASSWORD").ok(),
        ..SpanStoreSettings::at(url)
    }
}

#[tokio::test]
async fn opening_a_span_store_migrates_a_database_that_carries_no_schema() {
    let database = "span_store_opened";
    let Some(url) = an_empty_database(database).await else {
        return;
    };

    let sink = open_span_store(&settings_for(&url, database), FixedClock::at(NOW))
        .await
        .expect("the span store opens");
    sink.accept(
        TenantId::new(Uuid::from_u128(1)),
        vec![a_span(1, "checkout")],
    )
    .await
    .expect("a batch is accepted");
    sink.flush().await.expect("the batch commits");

    assert_eq!(
        stored_rows(&connected(&url).with_database(database)).await,
        1,
        "a store opened on an empty database must be ready to write"
    );
}

#[tokio::test]
async fn opening_a_span_store_twice_leaves_what_the_first_one_wrote() {
    let database = "span_store_reopened";
    let Some(url) = an_empty_database(database).await else {
        return;
    };
    let settings = settings_for(&url, database);

    let first = open_span_store(&settings, FixedClock::at(NOW))
        .await
        .expect("the first open");
    first
        .accept(
            TenantId::new(Uuid::from_u128(1)),
            vec![a_span(1, "checkout")],
        )
        .await
        .expect("a batch is accepted");
    first.flush().await.expect("the batch commits");

    let second = open_span_store(&settings, FixedClock::at(NOW))
        .await
        .expect("a second open must not refuse an applied schema");
    second
        .accept(
            TenantId::new(Uuid::from_u128(1)),
            vec![a_span(2, "checkout")],
        )
        .await
        .expect("a batch is accepted");
    second.flush().await.expect("the batch commits");

    assert_eq!(
        stored_rows(&connected(&url).with_database(database)).await,
        2,
        "reopening must migrate nothing and drop nothing"
    );
}
