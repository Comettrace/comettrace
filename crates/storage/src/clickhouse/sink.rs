use core::fmt;
use std::time::Duration;

use clickhouse::inserter::Inserter;
use clickhouse::{Client, error::Error as ClickhouseError};
use comettrace_core::{Clock, Scope, SinkError, Span, SpanSink};
use tokio::sync::Mutex;

use crate::clickhouse::rows::{EventRow, SpanRow};

const SPANS: &str = "spans";
const EVENTS: &str = "span_events";
const DEFAULT_MAX_ROWS: u64 = 100_000;
const DEFAULT_PERIOD: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchLimits {
    pub max_rows: u64,
    pub period: Duration,
}

impl Default for BatchLimits {
    fn default() -> Self {
        Self {
            max_rows: DEFAULT_MAX_ROWS,
            period: DEFAULT_PERIOD,
        }
    }
}

pub struct ClickhouseSink<C> {
    spans: Mutex<Inserter<SpanRow<'static>>>,
    /* A second inserter rather than a second table written from the first, because the two
    buffers fill at wildly different rates: a deployment whose spans carry no events should not
    be holding an empty events batch open, and one whose spans carry twenty each should not be
    committing spans twenty times as often. */
    events: Mutex<Inserter<EventRow<'static>>>,
    clock: C,
    retention: Duration,
    limits: BatchLimits,
}

impl<C> fmt::Debug for ClickhouseSink<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClickhouseSink")
            .field("retention", &self.retention)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl<C: Clock> ClickhouseSink<C> {
    pub fn new(client: &Client, limits: BatchLimits, retention: Duration, clock: C) -> Self {
        Self {
            spans: Mutex::new(inserter(client, SPANS, limits)),
            events: Mutex::new(inserter(client, EVENTS, limits)),
            clock,
            retention,
            limits,
        }
    }

    /* Both, and the spans first. If the second commit is refused the batch is lost either way,
    and spans without their events read as a trace that recorded none — which is wrong but
    legible. Events without their spans would be rows nothing can reach. */
    pub async fn flush(&self) -> Result<u64, SinkError> {
        let spans = self.spans.lock().await.force_commit().await;
        let events = self.events.lock().await.force_commit().await;
        let spans = spans.map_err(|error| refused(&error))?;
        let events = events.map_err(|error| refused(&error))?;
        Ok(spans.rows + events.rows)
    }

    pub async fn pending_rows(&self) -> u64 {
        self.spans.lock().await.pending().rows + self.events.lock().await.pending().rows
    }

    /* The sooner of the two, so the flush loop wakes for whichever deadline arrives first. */
    pub async fn time_left(&self) -> Option<Duration> {
        let spans = self.spans.lock().await.time_left();
        let events = self.events.lock().await.time_left();
        match (spans, events) {
            (Some(spans), Some(events)) => Some(spans.min(events)),
            (left, None) | (None, left) => left,
        }
    }
}

impl<C: Clock + Send + Sync> SpanSink for ClickhouseSink<C> {
    async fn accept(&self, scope: Scope, spans: Vec<Span>) -> Result<(), SinkError> {
        let expire_at = self
            .clock
            .now()
            .saturating_add_secs(self.retention.as_secs());

        {
            let mut inserter = self.spans.lock().await;
            for span in &spans {
                let row = SpanRow::of(scope, span, expire_at)?;
                inserter
                    .write(&row)
                    .await
                    .map_err(|error| refused(&error))?;
            }
            inserter.commit().await.map_err(|error| refused(&error))?;
        }

        /* Nothing is committed when there is nothing to commit. Most spans carry no events, and
        a deployment where none of them do should not be opening and closing an empty batch on
        every export. */
        if spans.iter().all(|span| span.events.is_empty()) {
            return Ok(());
        }

        let mut inserter = self.events.lock().await;
        for span in &spans {
            for event in &span.events {
                let row = EventRow::of(scope, span, event, expire_at)?;
                inserter
                    .write(&row)
                    .await
                    .map_err(|error| refused(&error))?;
            }
        }
        inserter.commit().await.map_err(|error| refused(&error))?;
        Ok(())
    }
}

fn inserter<R: clickhouse::Row>(client: &Client, table: &str, limits: BatchLimits) -> Inserter<R> {
    client
        .inserter::<R>(table)
        .with_max_rows(limits.max_rows)
        .with_period(Some(limits.period))
}

fn refused(error: &ClickhouseError) -> SinkError {
    tracing::error!(%error, "the span store refused a batch");
    SinkError::Unavailable
}
