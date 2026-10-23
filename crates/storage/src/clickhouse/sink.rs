use core::fmt;
use std::time::Duration;

use clickhouse::inserter::Inserter;
use clickhouse::{Client, error::Error as ClickhouseError};
use comettrace_core::{Clock, SinkError, Span, SpanSink, TenantId};
use tokio::sync::Mutex;

use crate::clickhouse::rows::SpanRow;

const TABLE: &str = "spans";
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
    inserter: Mutex<Inserter<SpanRow<'static>>>,
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
        let inserter = client
            .inserter::<SpanRow<'static>>(TABLE)
            .with_max_rows(limits.max_rows)
            .with_period(Some(limits.period));
        Self {
            inserter: Mutex::new(inserter),
            clock,
            retention,
            limits,
        }
    }

    pub async fn flush(&self) -> Result<u64, SinkError> {
        let mut inserter = self.inserter.lock().await;
        Ok(inserter.force_commit().await.map_err(refused)?.rows)
    }

    pub async fn pending_rows(&self) -> u64 {
        self.inserter.lock().await.pending().rows
    }

    pub async fn time_left(&self) -> Option<Duration> {
        self.inserter.lock().await.time_left()
    }
}

impl<C: Clock + Send + Sync> SpanSink for ClickhouseSink<C> {
    async fn accept(&self, tenant: TenantId, spans: Vec<Span>) -> Result<(), SinkError> {
        let expire_at = self
            .clock
            .now()
            .saturating_add_secs(self.retention.as_secs());
        let mut inserter = self.inserter.lock().await;
        for span in &spans {
            let row = SpanRow::of(tenant, span, expire_at)?;
            inserter.write(&row).await.map_err(refused)?;
        }
        inserter.commit().await.map_err(refused)?;
        Ok(())
    }
}

fn refused(error: ClickhouseError) -> SinkError {
    tracing::error!(%error, "the span store refused a batch");
    SinkError::Unavailable
}
