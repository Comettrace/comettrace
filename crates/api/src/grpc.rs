use std::sync::Arc;

use comettrace_core::{DecompressedBytes, Ingest, Payload, SpanSink, TenantDirectory, UsageMeter};
use comettrace_otlp::spans_of;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::{
    TraceService, TraceServiceServer,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use prost::Message;
use tonic::codec::CompressionEncoding;
use tonic::{Request, Response, Status};

use crate::auth;
use crate::error::ApiError;

#[derive(Debug)]
pub struct TraceGateway<D, S, M> {
    ingest: Arc<Ingest<D, S, M>>,
}

impl<D, S, M> TraceGateway<D, S, M> {
    pub fn new(ingest: Arc<Ingest<D, S, M>>) -> Self {
        Self { ingest }
    }
}

pub fn trace_server<D, S, M>(
    ingest: Arc<Ingest<D, S, M>>,
) -> TraceServiceServer<TraceGateway<D, S, M>>
where
    D: TenantDirectory + Send + Sync + 'static,
    S: SpanSink + Send + Sync + 'static,
    M: UsageMeter + Send + Sync + 'static,
{
    TraceServiceServer::new(TraceGateway::new(ingest)).accept_compressed(CompressionEncoding::Gzip)
}

#[tonic::async_trait]
impl<D, S, M> TraceService for TraceGateway<D, S, M>
where
    D: TenantDirectory + Send + Sync + 'static,
    S: SpanSink + Send + Sync + 'static,
    M: UsageMeter + Send + Sync + 'static,
{
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let key = auth::key_of(request.metadata())?;
        let message = request.into_inner();
        let decompressed = DecompressedBytes::new(message.encoded_len() as u64);
        let spans = spans_of(message).map_err(ApiError::from)?;
        self.ingest
            .accept(
                &key,
                Payload {
                    spans,
                    decompressed,
                },
            )
            .await
            .map_err(ApiError::from)?;
        Ok(Response::new(ExportTraceServiceResponse::default()))
    }
}
