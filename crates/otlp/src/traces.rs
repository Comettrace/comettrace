use comettrace_core::{InvalidId, Span, SpanEvent, SpanId, SpanKind, SpanStatus, TraceId};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::trace::v1::span::Event as ProtoEvent;
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
        events,
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
        events: events_of(events),
    })
}

/* The OTLP span limits default to 128 events per span, so a conforming sdk never sends more and
this only answers for one that does not. Rows are what events cost, and nothing upstream bounds
them: the quota counts decompressed bytes and the meter counts spans, so a single span carrying a
hundred thousand events would be storage nobody was charged for. Keeping the first is the right
end to keep — a span records them in order, and what a reader wants is where it started going
wrong, not where it gave up. */
const MOST_EVENTS: usize = 128;

fn events_of(events: Vec<ProtoEvent>) -> Vec<SpanEvent> {
    events
        .into_iter()
        .take(MOST_EVENTS)
        .map(|event| SpanEvent {
            time_unix_nanos: event.time_unix_nano,
            name: event.name.into_boxed_str(),
            attributes: attributes::collect(event.attributes),
        })
        .collect()
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
        DecodeError, ExportTraceServiceRequest, MOST_EVENTS, ProtoEvent, ProtoSpan, ResourceSpans,
        UNKNOWN_SERVICE, decode_traces,
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

    fn an_event(name: &str, at: u64, attributes: Vec<KeyValue>) -> ProtoEvent {
        ProtoEvent {
            time_unix_nano: at,
            name: name.to_owned(),
            attributes,
            ..ProtoEvent::default()
        }
    }

    #[test]
    fn an_exception_arrives_as_an_event_with_its_attributes() {
        let mut span = a_proto_span();
        span.events = vec![an_event(
            "exception",
            15,
            vec![
                text("exception.type", "PspTimeoutError"),
                text("exception.message", "deadline exceeded after 960ms"),
                text("exception.stacktrace", "at charge (psp/client.ts:88)"),
            ],
        )];
        let request = a_request(span, None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        let events = &spans[0].events;
        assert_eq!(events.len(), 1);
        assert_eq!(&*events[0].name, "exception");
        assert_eq!(events[0].time_unix_nanos, 15);
        assert_eq!(
            events[0].attributes.get("exception.type").map(|v| &**v),
            Some("PspTimeoutError")
        );
        assert_eq!(
            events[0]
                .attributes
                .get("exception.stacktrace")
                .map(|v| &**v),
            Some("at charge (psp/client.ts:88)")
        );
    }

    /* The order is the span's, because an exception is usually the last thing that happened and a
    reader walking down the list is walking forwards in time. */
    #[test]
    fn events_keep_the_order_the_span_recorded_them_in() {
        let mut span = a_proto_span();
        span.events = vec![
            an_event("retrying", 11, Vec::new()),
            an_event("exception", 19, Vec::new()),
        ];
        let request = a_request(span, None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        assert_eq!(
            spans[0]
                .events
                .iter()
                .map(|event| &*event.name)
                .collect::<Vec<_>>(),
            vec!["retrying", "exception"]
        );
    }

    #[test]
    fn a_span_with_no_events_carries_none() {
        let request = a_request(a_proto_span(), None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        assert!(spans[0].events.is_empty());
    }

    /* Rows are what events cost and nothing upstream bounds them, so the door does. A conforming
    sdk is under the limit already; this answers for one that is not. */
    #[test]
    fn a_span_carrying_more_events_than_the_limit_keeps_the_first_of_them() {
        let mut span = a_proto_span();
        span.events = (0..MOST_EVENTS + 40)
            .map(|at| an_event(&format!("step-{at}"), at as u64, Vec::new()))
            .collect();
        let request = a_request(span, None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        assert_eq!(spans[0].events.len(), MOST_EVENTS);
        assert_eq!(&*spans[0].events[0].name, "step-0");
        assert_eq!(
            &*spans[0].events[MOST_EVENTS - 1].name,
            &*format!("step-{}", MOST_EVENTS - 1)
        );
    }

    /* An event's attributes go through the same renderer as a span's, so a stack trace in bytes
    rather than text is dropped the same way and does not take the event with it. */
    #[test]
    fn an_event_attribute_that_does_not_render_is_dropped_and_the_event_stays() {
        let mut span = a_proto_span();
        span.events = vec![an_event(
            "exception",
            15,
            vec![
                text("exception.type", "PspTimeoutError"),
                pair("exception.stacktrace", Value::BytesValue(vec![1, 2])),
            ],
        )];
        let request = a_request(span, None);

        let spans = decode_traces(&request.encode_to_vec()).expect("a valid request");

        assert_eq!(spans[0].events.len(), 1);
        assert_eq!(spans[0].events[0].attributes.len(), 1);
        assert_eq!(
            spans[0].events[0].attributes.get("exception.stacktrace"),
            None
        );
    }
}
