use std::collections::BTreeMap;

use clickhouse::{Client, Row};
use comettrace_core::{
    ReadError, Span, SpanId, SpanKind, SpanReader, SpanStatus, TenantId, TraceFilter, TraceId,
    TraceOrder, TraceSummary,
};
use serde::Deserialize;

const SUMMARIES: &str = "SELECT \
     lower(hex(trace_id)) AS trace_id, \
     argMin(service_name, (parent_span_id IS NOT NULL, start_time)) AS root_service, \
     argMin(name, (parent_span_id IS NOT NULL, start_time)) AS root_name, \
     toUnixTimestamp64Nano(min(start_time)) AS started_at, \
     toUnixTimestamp64Nano(max(end_time)) - toUnixTimestamp64Nano(min(start_time)) AS duration, \
     count() AS spans, \
     countIf(status = 'error') AS errors \
   FROM spans \
   WHERE tenant_id = ? AND start_time >= ? AND start_time < ? \
   GROUP BY trace_id \
   HAVING (? = '' OR countIf(service_name = ?) > 0) \
     AND (? = 0 OR errors > 0) \
     AND duration >= ? \
   ORDER BY {order} DESC \
   LIMIT ?";

const ONE_TRACE: &str = "SELECT \
     lower(hex(span_id)) AS span_id, \
     lower(hex(parent_span_id)) AS parent_span_id, \
     service_name AS service_name, \
     name AS name, \
     toString(kind) AS kind, \
     toString(status) AS status, \
     toUnixTimestamp64Nano(start_time) AS start_time, \
     toUnixTimestamp64Nano(end_time) AS end_time, \
     attributes AS attributes \
   FROM spans \
   WHERE tenant_id = ? AND trace_id = unhex(?) \
   ORDER BY start_time, span_id";

#[derive(Debug, Clone)]
pub struct ClickhouseReader {
    client: Client,
}

#[derive(Debug, Row, Deserialize)]
struct SummaryRow {
    trace_id: String,
    root_service: String,
    root_name: String,
    started_at: i64,
    duration: i64,
    spans: u64,
    errors: u64,
}

#[derive(Debug, Row, Deserialize)]
struct SpanRow {
    span_id: String,
    parent_span_id: Option<String>,
    service_name: String,
    name: String,
    kind: String,
    status: String,
    start_time: i64,
    end_time: i64,
    attributes: Vec<(String, String)>,
}

impl ClickhouseReader {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

impl SpanReader for ClickhouseReader {
    async fn traces(
        &self,
        tenant: TenantId,
        filter: &TraceFilter,
    ) -> Result<Vec<TraceSummary>, ReadError> {
        let service = filter.service.as_deref().unwrap_or_default();
        let rows: Vec<SummaryRow> = self
            .client
            .query(&SUMMARIES.replace("{order}", ordering(filter.order)))
            .bind(tenant.as_uuid())
            .bind(nanos(filter.window.from_unix_nanos()))
            .bind(nanos(filter.window.to_unix_nanos()))
            .bind(service)
            .bind(service)
            .bind(u8::from(filter.only_errors))
            .bind(nanos(filter.at_least_nanos.unwrap_or_default()))
            .bind(filter.limit())
            .fetch_all()
            .await
            .map_err(|error| refused(&error))?;

        rows.into_iter().map(read_summary).collect()
    }

    async fn trace(&self, tenant: TenantId, id: TraceId) -> Result<Vec<Span>, ReadError> {
        let rows: Vec<SpanRow> = self
            .client
            .query(ONE_TRACE)
            .bind(tenant.as_uuid())
            .bind(hex::encode(id.as_bytes()))
            .fetch_all()
            .await
            .map_err(|error| refused(&error))?;

        rows.into_iter().map(|row| read_span(id, row)).collect()
    }
}

fn ordering(order: TraceOrder) -> &'static str {
    match order {
        TraceOrder::Started => "started_at",
        TraceOrder::Duration => "duration",
        TraceOrder::Spans => "spans",
    }
}

fn read_summary(row: SummaryRow) -> Result<TraceSummary, ReadError> {
    Ok(TraceSummary {
        trace_id: a_trace(&row.trace_id)?,
        root_service: row.root_service.into(),
        root_name: row.root_name.into(),
        started_unix_nanos: unsigned(row.started_at),
        duration_nanos: unsigned(row.duration),
        spans: row.spans,
        errors: row.errors,
    })
}

fn read_span(trace: TraceId, row: SpanRow) -> Result<Span, ReadError> {
    let parent = match row.parent_span_id.as_deref() {
        None | Some("") => None,
        Some(spelling) => Some(a_span(spelling)?),
    };
    Ok(Span {
        trace_id: trace,
        span_id: a_span(&row.span_id)?,
        parent,
        name: row.name.into(),
        service_name: row.service_name.into(),
        kind: read_kind(&row.kind),
        status: read_status(&row.status),
        start_unix_nanos: unsigned(row.start_time),
        end_unix_nanos: unsigned(row.end_time),
        attributes: row
            .attributes
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect::<BTreeMap<_, _>>(),
    })
}

fn a_trace(spelling: &str) -> Result<TraceId, ReadError> {
    TraceId::try_from(decoded(spelling)?.as_slice()).map_err(|error| held(&error))
}

fn a_span(spelling: &str) -> Result<SpanId, ReadError> {
    SpanId::try_from(decoded(spelling)?.as_slice()).map_err(|error| held(&error))
}

fn decoded(spelling: &str) -> Result<Vec<u8>, ReadError> {
    hex::decode(spelling).map_err(|error| held(&error))
}

fn held(error: &dyn core::fmt::Display) -> ReadError {
    tracing::error!(%error, "the span store holds an identifier we cannot read");
    ReadError::StoreUnreachable
}

fn read_kind(spelling: &str) -> SpanKind {
    match spelling {
        "internal" => SpanKind::Internal,
        "server" => SpanKind::Server,
        "client" => SpanKind::Client,
        "producer" => SpanKind::Producer,
        "consumer" => SpanKind::Consumer,
        _ => SpanKind::Unspecified,
    }
}

fn read_status(spelling: &str) -> SpanStatus {
    match spelling {
        "ok" => SpanStatus::Ok,
        "error" => SpanStatus::Error,
        _ => SpanStatus::Unset,
    }
}

fn nanos(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn refused(error: &clickhouse::error::Error) -> ReadError {
    tracing::error!(%error, "the span store refused a query");
    ReadError::StoreUnreachable
}

#[cfg(test)]
mod tests {
    use comettrace_core::{SpanKind, SpanStatus, TraceOrder};

    use super::{ONE_TRACE, SUMMARIES, ordering, read_kind, read_status};

    #[test]
    fn every_kind_the_schema_holds_is_read_back() {
        assert_eq!(read_kind("server"), SpanKind::Server);
        assert_eq!(read_kind("client"), SpanKind::Client);
        assert_eq!(read_kind("internal"), SpanKind::Internal);
        assert_eq!(read_kind("producer"), SpanKind::Producer);
        assert_eq!(read_kind("consumer"), SpanKind::Consumer);
        assert_eq!(read_kind("unspecified"), SpanKind::Unspecified);
    }

    #[test]
    fn every_status_the_schema_holds_is_read_back() {
        assert_eq!(read_status("ok"), SpanStatus::Ok);
        assert_eq!(read_status("error"), SpanStatus::Error);
        assert_eq!(read_status("unset"), SpanStatus::Unset);
    }

    #[test]
    fn a_kind_or_status_we_do_not_know_falls_back_rather_than_failing() {
        assert_eq!(read_kind("whatever"), SpanKind::Unspecified);
        assert_eq!(read_status("whatever"), SpanStatus::Unset);
    }

    #[test]
    fn every_ordering_names_a_column_the_query_selects() {
        for order in [TraceOrder::Started, TraceOrder::Duration, TraceOrder::Spans] {
            let column = ordering(order);
            assert!(
                SUMMARIES.contains(&format!("AS {column}")),
                "{column} is not selected"
            );
        }
    }

    #[test]
    fn neither_query_reads_across_tenants() {
        for query in [SUMMARIES, ONE_TRACE] {
            assert!(
                query.contains("WHERE tenant_id = ?"),
                "a query without a tenant would read somebody else's spans"
            );
        }
    }
}
