use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::header::COOKIE;
use axum::http::{Request, StatusCode};
use comettrace_api::explore_router;
use comettrace_core::{
    AccountId, Explore, Membership, Memberships, OrgRole, ReadError, Session, SessionError,
    SessionStore, SessionToken, Span, SpanId, SpanKind, SpanReader, SpanStatus, TenantId,
    TraceFilter, TraceId, TraceSummary, UnixSeconds, Whoami, WhoamiError,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "ct_ss_0123456789";
const MINE: u128 = 1;
const THEIRS: u128 = 2;
const TRACE: &str = "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a";

struct OneSession;

impl SessionStore for OneSession {
    async fn issue(
        &self,
        _account: AccountId,
        _org: Option<TenantId>,
    ) -> Result<SessionToken, SessionError> {
        Ok(SessionToken::from_plaintext(TOKEN))
    }

    async fn resolve(&self, token: &SessionToken) -> Result<Session, SessionError> {
        if token.exposed() == TOKEN {
            Ok(Session {
                account: AccountId::new(Uuid::from_u128(9)),
                org: None,
                expires_at: UnixSeconds::new(0),
            })
        } else {
            Err(SessionError::Unknown)
        }
    }

    async fn revoke(&self, _token: &SessionToken) -> Result<(), SessionError> {
        Ok(())
    }
}

struct MemberOfMine;

impl Memberships for MemberOfMine {
    async fn of(&self, _account: AccountId) -> Result<Vec<Membership>, WhoamiError> {
        Ok(vec![Membership {
            org: TenantId::new(Uuid::from_u128(MINE)),
            role: OrgRole::Member,
        }])
    }
}

struct TwoSpans;

fn a_trace_id() -> TraceId {
    TraceId::try_from(hex::decode(TRACE).expect("hex").as_slice()).expect("sixteen bytes")
}

fn a_span(seed: u8, parent: Option<u8>, start: u64, end: u64) -> Span {
    Span {
        trace_id: a_trace_id(),
        span_id: SpanId::try_from([seed; 8].as_slice()).expect("eight bytes"),
        parent: parent.map(|id| SpanId::try_from([id; 8].as_slice()).expect("eight bytes")),
        name: "work".into(),
        service_name: "checkout".into(),
        kind: SpanKind::Server,
        status: SpanStatus::Ok,
        start_unix_nanos: start,
        end_unix_nanos: end,
        attributes: BTreeMap::from([("http.status_code".into(), "200".into())]),
    }
}

impl SpanReader for TwoSpans {
    async fn traces(
        &self,
        _tenant: TenantId,
        _filter: &TraceFilter,
    ) -> Result<Vec<TraceSummary>, ReadError> {
        Ok(vec![TraceSummary {
            trace_id: a_trace_id(),
            root_service: "checkout".into(),
            root_name: "pay".into(),
            started_unix_nanos: 1_700_000_000_123_456_789,
            duration_nanos: 2_500_000,
            spans: 2,
            errors: 1,
        }])
    }

    async fn trace(&self, _tenant: TenantId, _id: TraceId) -> Result<Vec<Span>, ReadError> {
        Ok(vec![
            a_span(
                1,
                None,
                1_700_000_000_000_000_000,
                1_700_000_002_000_000_000,
            ),
            a_span(
                2,
                Some(1),
                1_700_000_000_500_000_000,
                1_700_000_001_000_000_000,
            ),
        ])
    }
}

struct NoSpans;

impl SpanReader for NoSpans {
    async fn traces(
        &self,
        _tenant: TenantId,
        _filter: &TraceFilter,
    ) -> Result<Vec<TraceSummary>, ReadError> {
        Ok(Vec::new())
    }

    async fn trace(&self, _tenant: TenantId, _id: TraceId) -> Result<Vec<Span>, ReadError> {
        Ok(Vec::new())
    }
}

fn looking(uri: &str, cookie: Option<&str>, scripted: bool) -> Request<Body> {
    let mut request = Request::builder().uri(uri);
    if scripted {
        request = request.header("x-requested-with", "fetch");
    }
    if let Some(cookie) = cookie {
        request = request.header(COOKIE, cookie);
    }
    request.body(Body::empty()).expect("a request")
}

async fn served<R>(reader: R, request: Request<Body>) -> (StatusCode, String)
where
    R: SpanReader + Send + Sync + 'static,
{
    let explore = Arc::new(Explore::new(Whoami::new(OneSession, MemberOfMine), reader));
    let response = explore_router(explore)
        .oneshot(request)
        .await
        .expect("a response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    (status, String::from_utf8(body.to_vec()).expect("text"))
}

fn org(seed: u128) -> String {
    Uuid::from_u128(seed).to_string()
}

fn session() -> String {
    format!("ct_session={TOKEN}")
}

#[tokio::test]
async fn a_member_lists_the_traces_of_its_own_organisation() {
    let uri = format!(
        "/api/orgs/{}/traces?from=1700000000000&to=1700000100000",
        org(MINE)
    );

    let (status, body) = served(TwoSpans, looking(&uri, Some(&session()), true)).await;

    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_str(&body).expect("json");
    let listed = &found["traces"][0];
    assert_eq!(listed["trace_id"], TRACE);
    assert_eq!(listed["root_service"], "checkout");
    assert_eq!(listed["spans"], 2);
    assert_eq!(listed["errors"], 1);
    assert_eq!(
        listed["started_at_ms"], 1_700_000_000_123u64,
        "a time a browser can hold without losing digits"
    );
    assert_eq!(listed["duration_us"], 2500);
}

#[tokio::test]
async fn an_organisation_this_session_does_not_belong_to_is_not_found() {
    let uri = format!("/api/orgs/{}/traces?from=1&to=2", org(THEIRS));

    let (status, _) = served(TwoSpans, looking(&uri, Some(&session()), true)).await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "telling a stranger an organisation exists is itself a leak"
    );
}

#[tokio::test]
async fn a_request_without_a_session_is_refused() {
    let uri = format!("/api/orgs/{}/traces?from=1&to=2", org(MINE));

    let (no_cookie, _) = served(TwoSpans, looking(&uri, None, true)).await;
    let (unknown, _) = served(TwoSpans, looking(&uri, Some("ct_session=ct_ss_nope"), true)).await;
    let (unscripted, _) = served(TwoSpans, looking(&uri, Some(&session()), false)).await;

    assert_eq!(no_cookie, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown, StatusCode::UNAUTHORIZED);
    assert_eq!(unscripted, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_window_that_ends_before_it_starts_is_refused() {
    let uri = format!("/api/orgs/{}/traces?from=20&to=10", org(MINE));

    let (status, body) = served(TwoSpans, looking(&uri, Some(&session()), true)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("must end after it starts"), "{body}");
}

#[tokio::test]
async fn a_window_nobody_gave_is_refused() {
    let uri = format!("/api/orgs/{}/traces", org(MINE));

    let (status, _) = served(TwoSpans, looking(&uri, Some(&session()), true)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn one_trace_is_laid_out_for_a_waterfall() {
    let uri = format!("/api/orgs/{}/traces/{TRACE}", org(MINE));

    let (status, body) = served(TwoSpans, looking(&uri, Some(&session()), true)).await;

    assert_eq!(status, StatusCode::OK);
    let trace: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(trace["trace_id"], TRACE);
    assert_eq!(trace["duration_us"], 2_000_000);
    let root = &trace["spans"][0];
    assert_eq!(root["start_offset_us"], 0, "the root anchors the waterfall");
    assert_eq!(root["duration_us"], 2_000_000);
    assert_eq!(root["parent_span_id"], serde_json::Value::Null);
    assert_eq!(root["kind"], "server");
    assert_eq!(root["status"], "ok");
    assert_eq!(root["attributes"]["http.status_code"], "200");
    let child = &trace["spans"][1];
    assert_eq!(child["start_offset_us"], 500_000);
    assert_eq!(child["duration_us"], 500_000);
    assert_eq!(child["parent_span_id"], "0101010101010101");
}

#[tokio::test]
async fn a_trace_identifier_that_is_not_one_is_refused() {
    for candidate in ["nothex", "0a0a", "00000000000000000000000000000000"] {
        let uri = format!("/api/orgs/{}/traces/{candidate}", org(MINE));

        let (status, _) = served(TwoSpans, looking(&uri, Some(&session()), true)).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{candidate} should not reach the store"
        );
    }
}

#[tokio::test]
async fn a_trace_the_store_does_not_hold_is_not_found() {
    let uri = format!("/api/orgs/{}/traces/{TRACE}", org(MINE));

    let (status, _) = served(NoSpans, looking(&uri, Some(&session()), true)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_empty_list_is_an_answer_rather_than_an_error() {
    let uri = format!("/api/orgs/{}/traces?from=1&to=2", org(MINE));

    let (status, body) = served(NoSpans, looking(&uri, Some(&session()), true)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"traces":[]}"#);
}
