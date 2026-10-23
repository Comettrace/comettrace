use comettrace_core::{InvalidId, Span, SpanId, SpanKind, SpanStatus, TraceId};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::trace::v1::span::SpanKind as ProtoKind;
use opentelemetry_proto::tonic::trace::v1::status::StatusCode;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, Span as ProtoSpan};
use prost::Message;

use crate::attributes;

pub const UNKNOWN_SERVICE: &str = "unknown_service";
const SERVICE_NAME: &str = "service.name";

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("the payload is not a valid OTLP trace export request")]
    Protobuf(#[from] prost::DecodeError),
    #[error(transparent)]
    Id(#[from] InvalidId),
}

pub fn decode_traces(payload: &[u8]) -> Result<Vec<Span>, DecodeError> {
    spans_of(ExportTraceServiceRequest::decode(payload)?)
}

pub fn spans_of(request: ExportTraceServiceRequest) -> Result<Vec<Span>, DecodeError> {
    let mut spans = Vec::with_capacity(span_count(&request.resource_spans));
    for resource_spans in request.resource_spans {
        let service = service_name(&resource_spans);
        for scope_spans in resource_spans.scope_spans {
            for span in scope_spans.spans {
                spans.push(convert(span, &service)?);
            }
        }
    }
    Ok(spans)
}

fn span_count(resource_spans: &[ResourceSpans]) -> usize {
    resource_spans
        .iter()
        .flat_map(|resource| resource.scope_spans.iter())
        .map(|scope| scope.spans.len())
        .sum()
}

fn service_name(resource_spans: &ResourceSpans) -> Box<str> {
    resource_spans
        .resource
        .as_ref()
        .and_then(|resource| attributes::lookup(&resource.attributes, SERVICE_NAME))
        .unwrap_or_else(|| UNKNOWN_SERVICE.into())
}

fn convert(span: ProtoSpan, service: &str) -> Result<Span, DecodeError> {
    let ProtoSpan {
        trace_id,
        span_id,
        parent_span_id,
        name,
        kind,
        start_time_unix_nano,
        end_time_unix_nano,
        attributes: pairs,
        status,
        ..
    } = span;

    let parent = if parent_span_id.is_empty() {
        None
    } else {
        Some(SpanId::try_from(parent_span_id.as_slice())?)
    };

    Ok(Span {
        trace_id: TraceId::try_from(trace_id.as_slice())?,
        span_id: SpanId::try_from(span_id.as_slice())?,
        parent,
        name: name.into_boxed_str(),
        service_name: service.into(),
        kind: kind_of(kind),
        status: status_of(status.map(|status| status.code)),
        start_unix_nanos: start_time_unix_nano,
        end_unix_nanos: end_time_unix_nano,
        attributes: attributes::collect(pairs),
    })
}

fn kind_of(code: i32) -> SpanKind {
    match ProtoKind::try_from(code) {
        Ok(ProtoKind::Internal) => SpanKind::Internal,
        Ok(ProtoKind::Server) => SpanKind::Server,
        Ok(ProtoKind::Client) => SpanKind::Client,
        Ok(ProtoKind::Producer) => SpanKind::Producer,
        Ok(ProtoKind::Consumer) => SpanKind::Consumer,
        Ok(ProtoKind::Unspecified) | Err(_) => SpanKind::Unspecified,
    }
}

fn status_of(code: Option<i32>) -> SpanStatus {
    match code.map(StatusCode::try_from) {
        Some(Ok(StatusCode::Ok)) => SpanStatus::Ok,
        Some(Ok(StatusCode::Error)) => SpanStatus::Error,
        Some(Ok(StatusCode::Unset) | Err(_)) | None => SpanStatus::Unset,
    }
}

#[cfg(test)]
mod tests {
    use comettrace_core::{SpanKind, SpanStatus};
    use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue, any_value::Value};
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use opentelemetry_proto::tonic::trace::v1::{ScopeSpans, Status};
    use prost::Message;

    use super::{
        DecodeError, ExportTraceServiceRequest, ProtoSpan, ResourceSpans, UNKNOWN_SERVICE,
        decode_traces,
    };

    fn pair(key: &str, value: Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
            ..KeyValue::default()
        }
    }

    fn text(key: &str, value: &str) -> KeyValue {
        pair(key, Value::StringValue(value.to_owned()))
    }

    fn a_proto_span() -> ProtoSpan {
        ProtoSpan {
            trace_id: vec![9u8; 16],
            span_id: vec![4u8; 8],
            parent_span_id: Vec::new(),
            name: "GET /orders".to_owned(),
            kind: 2,
            start_time_unix_nano: 10,
            end_time_unix_nano: 20,
            status: Some(Status {
                code: 1,
                ..Status::default()
            }),
            ..ProtoSpan::default()
        }
    }

    fn a_request(span: ProtoSpan, resource: Option<Resource>) -> ExportTraceServiceRequest {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource,
                scope_spans: vec![ScopeSpans {
                    spans: vec![span],
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            }],
        }
    }

    fn a_resource(attributes: Vec<KeyValue>) -> Resource {
        Resource {
            attributes,
            ..Resource::default()
        }
    }

    #[test]
    fn a_request_carries_its_spans_and_service_into_the_domain() {
        let request = a_request(
            a_proto_span(),
            Some(a_resource(vec![text("service.name", "orders")])),
        );

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(&*span.service_name, "orders");
        assert_eq!(&*span.name, "GET /orders");
        assert_eq!(span.kind, SpanKind::Server);
        assert_eq!(span.status, SpanStatus::Ok);
        assert_eq!(span.start_unix_nanos, 10);
        assert_eq!(span.end_unix_nanos, 20);
        assert_eq!(span.parent, None);
    }

    #[test]
    fn a_resource_without_a_service_name_falls_back() {
        let request = a_request(a_proto_span(), None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        assert_eq!(&*spans[0].service_name, UNKNOWN_SERVICE);
    }

    #[test]
    fn an_all_zero_trace_id_is_refused() {
        let mut span = a_proto_span();
        span.trace_id = vec![0u8; 16];
        let request = a_request(span, None);

        let outcome = decode_traces(&request.encode_to_vec());

        assert!(matches!(outcome, Err(DecodeError::Id(_))));
    }

    #[test]
    fn a_truncated_payload_is_refused() {
        let outcome = decode_traces(&[0xff, 0xff, 0xff]);

        assert!(matches!(outcome, Err(DecodeError::Protobuf(_))));
    }

    #[test]
    fn typed_attributes_become_text_and_unsupported_ones_are_dropped() {
        let mut span = a_proto_span();
        span.attributes = vec![
            text("http.route", "/orders"),
            pair("http.status_code", Value::IntValue(200)),
            pair("retried", Value::BoolValue(true)),
            pair("payload", Value::BytesValue(vec![1, 2])),
        ];
        let request = a_request(span, None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        let attributes = &spans[0].attributes;
        assert_eq!(attributes.get("http.route").map(|v| &**v), Some("/orders"));
        assert_eq!(
            attributes.get("http.status_code").map(|v| &**v),
            Some("200")
        );
        assert_eq!(attributes.get("retried").map(|v| &**v), Some("true"));
        assert_eq!(attributes.get("payload"), None);
    }

    #[test]
    fn a_parent_span_id_is_kept() {
        let mut span = a_proto_span();
        span.parent_span_id = vec![5u8; 8];
        let request = a_request(span, None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        assert_eq!(spans[0].parent.map(|id| *id.as_bytes()), Some([5u8; 8]));
    }
}
