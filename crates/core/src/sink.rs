use core::future::Future;

use crate::span::Span;
use crate::tenant::TenantId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SinkError {
    #[error("the span sink is unavailable")]
    Unavailable,
    #[error("the sink accepts at most {limit} spans per batch")]
    BatchTooLarge { limit: usize },
}

pub trait SpanSink {
    fn accept(
        &self,
        tenant: TenantId,
        spans: Vec<Span>,
    ) -> impl Future<Output = Result<(), SinkError>> + Send;
}

impl<T: SpanSink + Sync> SpanSink for &T {
    fn accept(
        &self,
        tenant: TenantId,
        spans: Vec<Span>,
    ) -> impl Future<Output = Result<(), SinkError>> + Send {
        (*self).accept(tenant, spans)
    }
}

impl<T: SpanSink + Send + Sync> SpanSink for std::sync::Arc<T> {
    fn accept(
        &self,
        tenant: TenantId,
        spans: Vec<Span>,
    ) -> impl Future<Output = Result<(), SinkError>> + Send {
        self.as_ref().accept(tenant, spans)
    }
}
