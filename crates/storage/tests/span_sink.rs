use std::collections::BTreeMap;
use std::time::Duration;

use clickhouse::Client;
use comettrace_core::{
    Span, SpanId, SpanKind, SpanReader, SpanSink, SpanStatus, TenantId, TraceFilter, TraceId,
    Window,
};
use comettrace_storage::{
    BatchLimits, ClickhouseMigrator, ClickhouseReader, ClickhouseSink, FixedClock, MemorySink,
    SpanStoreSettings, open_span_store,
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

    let store = open_span_store(&settings_for(&url, database), FixedClock::at(NOW))
        .await
        .expect("the span store opens");
    let sink = store.sink;
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
        .expect("the first open")
        .sink;
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
        .expect("a second open must not refuse an applied schema")
        .sink;
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

fn a_child(seed: u8, service: &str, parent: u8, start: u64, end: u64) -> Span {
    Span {
        parent: Some(SpanId::try_from([parent; 8].as_slice()).expect("eight bytes")),
        service_name: service.into(),
        start_unix_nanos: start,
        end_unix_nanos: end,
        ..a_span(seed, service)
    }
}

async fn a_written_store(database: &str) -> Option<(String, ClickhouseReader)> {
    let url = an_empty_database(database).await?;
    let store = open_span_store(&settings_for(&url, database), FixedClock::at(NOW))
        .await
        .expect("the span store opens");
    Some((url, store.reader))
}

#[tokio::test]
async fn a_written_trace_is_read_back_whole_and_in_order() {
    let Some((url, reader)) = a_written_store("read_one_trace").await else {
        return;
    };
    let store = open_span_store(&settings_for(&url, "read_one_trace"), FixedClock::at(NOW))
        .await
        .expect("a sink")
        .sink;
    let tenant = TenantId::new(Uuid::from_u128(1));
    let root = a_span(1, "checkout");
    let child = a_child(
        2,
        "payments",
        1,
        root.start_unix_nanos + 1_000,
        root.end_unix_nanos,
    );
    let trace = root.trace_id;
    store
        .accept(tenant, vec![child.clone(), root.clone()])
        .await
        .expect("a batch is accepted");
    store.flush().await.expect("the batch commits");

    let found = reader.trace(tenant, trace).await.expect("the trace reads");

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].span_id, root.span_id, "the root comes first");
    assert_eq!(found[0].parent, None);
    assert_eq!(found[1].parent, Some(root.span_id));
    assert_eq!(&*found[1].service_name, "payments");
    assert_eq!(found[0].kind, root.kind);
    assert_eq!(found[0].status, root.status);
    assert_eq!(
        found[0].attributes.get("http.status_code").map(|v| &**v),
        Some("200"),
        "attributes survive the round trip"
    );
}

#[tokio::test]
async fn a_trace_belonging_to_another_tenant_is_not_read() {
    let Some((url, reader)) = a_written_store("read_other_tenant").await else {
        return;
    };
    let store = open_span_store(
        &settings_for(&url, "read_other_tenant"),
        FixedClock::at(NOW),
    )
    .await
    .expect("a sink")
    .sink;
    let mine = TenantId::new(Uuid::from_u128(1));
    let theirs = TenantId::new(Uuid::from_u128(2));
    let span = a_span(1, "checkout");
    store
        .accept(mine, vec![span.clone()])
        .await
        .expect("a batch is accepted");
    store.flush().await.expect("the batch commits");

    assert_eq!(
        reader
            .trace(mine, span.trace_id)
            .await
            .expect("a read")
            .len(),
        1
    );
    assert!(
        reader
            .trace(theirs, span.trace_id)
            .await
            .expect("a read")
            .is_empty(),
        "a trace id is not a capability: another tenant must see nothing"
    );
}

#[tokio::test]
async fn a_trace_nobody_wrote_reads_as_empty() {
    let Some((_, reader)) = a_written_store("read_absent_trace").await else {
        return;
    };

    let found = reader
        .trace(
            TenantId::new(Uuid::from_u128(1)),
            TraceId::try_from([9u8; 16].as_slice()).expect("sixteen bytes"),
        )
        .await
        .expect("a read");

    assert!(found.is_empty());
}

#[tokio::test]
async fn the_trace_list_summarises_each_trace_once() {
    let Some((url, reader)) = a_written_store("read_trace_list").await else {
        return;
    };
    let store = open_span_store(&settings_for(&url, "read_trace_list"), FixedClock::at(NOW))
        .await
        .expect("a sink")
        .sink;
    let tenant = TenantId::new(Uuid::from_u128(1));
    let root = a_span(1, "checkout");
    let child = a_child(
        2,
        "payments",
        1,
        root.start_unix_nanos + 1_000,
        root.end_unix_nanos,
    );
    store
        .accept(tenant, vec![root.clone(), child])
        .await
        .expect("a batch is accepted");
    store.flush().await.expect("the batch commits");

    let found = reader
        .traces(tenant, &a_wide_filter())
        .await
        .expect("the list reads");

    assert_eq!(found.len(), 1, "two spans of one trace are one row");
    let summary = &found[0];
    assert_eq!(summary.trace_id, root.trace_id);
    assert_eq!(
        &*summary.root_service, "checkout",
        "the parentless span names the trace"
    );
    assert_eq!(summary.spans, 2);
    assert_eq!(summary.errors, 0);
    assert_eq!(summary.started_unix_nanos, root.start_unix_nanos);
    assert_eq!(
        summary.duration_nanos,
        root.end_unix_nanos - root.start_unix_nanos
    );
}

#[tokio::test]
async fn the_trace_list_counts_errors_and_can_keep_only_traces_that_have_them() {
    let Some((url, reader)) = a_written_store("read_trace_errors").await else {
        return;
    };
    let store = open_span_store(
        &settings_for(&url, "read_trace_errors"),
        FixedClock::at(NOW),
    )
    .await
    .expect("a sink")
    .sink;
    let tenant = TenantId::new(Uuid::from_u128(1));
    let healthy = a_span(1, "checkout");
    let mut broken = a_span(3, "checkout");
    broken.status = SpanStatus::Error;
    store
        .accept(tenant, vec![healthy.clone(), broken.clone()])
        .await
        .expect("a batch is accepted");
    store.flush().await.expect("the batch commits");

    let all = reader
        .traces(tenant, &a_wide_filter())
        .await
        .expect("the list reads");
    let mut only_errors = a_wide_filter();
    only_errors.only_errors = true;
    let failing = reader
        .traces(tenant, &only_errors)
        .await
        .expect("the list reads");

    assert_eq!(
        all.len(),
        2,
        "two spans with different trace ids are two traces"
    );
    assert_eq!(failing.len(), 1);
    assert_eq!(failing[0].trace_id, broken.trace_id);
    assert_eq!(failing[0].errors, 1);
}

#[tokio::test]
async fn the_trace_list_can_be_narrowed_to_a_service_that_appears_anywhere_in_it() {
    let Some((url, reader)) = a_written_store("read_trace_service").await else {
        return;
    };
    let store = open_span_store(
        &settings_for(&url, "read_trace_service"),
        FixedClock::at(NOW),
    )
    .await
    .expect("a sink")
    .sink;
    let tenant = TenantId::new(Uuid::from_u128(1));
    let root = a_span(1, "checkout");
    let child = a_child(
        2,
        "payments",
        1,
        root.start_unix_nanos + 1_000,
        root.end_unix_nanos,
    );
    store
        .accept(tenant, vec![root.clone(), child])
        .await
        .expect("a batch is accepted");
    store.flush().await.expect("the batch commits");

    let mut deep = a_wide_filter();
    deep.service = Some("payments".into());
    let mut absent = a_wide_filter();
    absent.service = Some("nobody".into());

    let found = reader.traces(tenant, &deep).await.expect("a read");

    assert_eq!(
        found.len(),
        1,
        "a service that is not the root still finds its trace"
    );
    assert_eq!(&*found[0].root_service, "checkout");
    assert_eq!(
        found[0].spans, 2,
        "the whole trace is summarised, not the match"
    );
    assert!(
        reader
            .traces(tenant, &absent)
            .await
            .expect("a read")
            .is_empty()
    );
}

#[tokio::test]
async fn a_window_that_misses_the_spans_finds_nothing() {
    let Some((url, reader)) = a_written_store("read_trace_window").await else {
        return;
    };
    let store = open_span_store(
        &settings_for(&url, "read_trace_window"),
        FixedClock::at(NOW),
    )
    .await
    .expect("a sink")
    .sink;
    let tenant = TenantId::new(Uuid::from_u128(1));
    let span = a_span(1, "checkout");
    store
        .accept(tenant, vec![span.clone()])
        .await
        .expect("a batch is accepted");
    store.flush().await.expect("the batch commits");

    let before = TraceFilter::over(Window::new(1, span.start_unix_nanos).expect("a window"));
    let after = TraceFilter::over(
        Window::new(span.end_unix_nanos + 1, span.end_unix_nanos + 1_000).expect("a window"),
    );

    assert!(
        reader
            .traces(tenant, &before)
            .await
            .expect("a read")
            .is_empty()
    );
    assert!(
        reader
            .traces(tenant, &after)
            .await
            .expect("a read")
            .is_empty()
    );
}

#[tokio::test]
async fn the_trace_list_never_crosses_tenants() {
    let Some((url, reader)) = a_written_store("read_trace_tenants").await else {
        return;
    };
    let store = open_span_store(
        &settings_for(&url, "read_trace_tenants"),
        FixedClock::at(NOW),
    )
    .await
    .expect("a sink")
    .sink;
    let mine = TenantId::new(Uuid::from_u128(1));
    let theirs = TenantId::new(Uuid::from_u128(2));
    store
        .accept(mine, vec![a_span(1, "checkout")])
        .await
        .expect("a batch is accepted");
    store
        .accept(theirs, vec![a_span(2, "billing")])
        .await
        .expect("a batch is accepted");
    store.flush().await.expect("the batch commits");

    let ours = reader.traces(mine, &a_wide_filter()).await.expect("a read");

    assert_eq!(ours.len(), 1);
    assert_eq!(&*ours[0].root_service, "checkout");
}

fn a_wide_filter() -> TraceFilter {
    TraceFilter::over(Window::new(1, u64::MAX / 2).expect("a window"))
}
