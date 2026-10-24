use crate::key::IngestKey;
use crate::quota::{Admission, QuotaPolicy};
use crate::sink::{SinkError, SpanSink};
use crate::span::Span;
use crate::tenant::{TenantDirectory, TenantId, UnknownKey};
use crate::usage::{DecompressedBytes, UsageMeter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub spans: Vec<Span>,
    pub decompressed: DecompressedBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IngestError {
    #[error(transparent)]
    UnknownKey(#[from] UnknownKey),
    #[error("the tenant has used its allowance of {} bytes", allowance.get())]
    Capped { allowance: DecompressedBytes },
    #[error(transparent)]
    Sink(#[from] SinkError),
}

#[derive(Debug, Clone)]
pub struct Ingest<D, Q, S, M> {
    directory: D,
    quota: Q,
    sink: S,
    meter: M,
}

impl<D, Q, S, M> Ingest<D, Q, S, M>
where
    D: TenantDirectory,
    Q: QuotaPolicy,
    S: SpanSink,
    M: UsageMeter,
{
    pub fn new(directory: D, quota: Q, sink: S, meter: M) -> Self {
        Self {
            directory,
            quota,
            sink,
            meter,
        }
    }

    pub async fn accept(&self, key: &IngestKey, payload: Payload) -> Result<TenantId, IngestError> {
        let tenant = self.directory.resolve(key).await?;
        if let Admission::Capped { allowance } =
            self.quota.admit(tenant, payload.decompressed).await
        {
            return Err(IngestError::Capped { allowance });
        }
        self.meter.record_ingest(tenant, payload.decompressed).await;
        self.sink.accept(tenant, payload.spans).await?;
        Ok(tenant)
    }
}
#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::future::Future;
    use std::sync::Mutex;

    use uuid::Uuid;

    use super::{Ingest, IngestError, Payload};
    use crate::key::IngestKey;
    use crate::quota::{Admission, QuotaPolicy, Unmetered};
    use crate::sink::{SinkError, SpanSink};
    use crate::span::{Span, SpanId, SpanKind, SpanStatus, TraceId};
    use crate::tenant::{TenantDirectory, TenantId, UnknownKey};
    use crate::usage::{DecompressedBytes, UsageMeter};

    #[derive(Debug, Default)]
    struct FakeDirectory(HashMap<String, TenantId>);

    impl FakeDirectory {
        fn holding(plaintext: &str, tenant: TenantId) -> Self {
            Self(HashMap::from([(plaintext.to_owned(), tenant)]))
        }
    }

    impl TenantDirectory for FakeDirectory {
        fn resolve(
            &self,
            key: &IngestKey,
        ) -> impl Future<Output = Result<TenantId, UnknownKey>> + Send {
            let found = self.0.get(key.exposed()).copied().ok_or(UnknownKey);
            async move { found }
        }
    }

    #[derive(Debug, Default)]
    struct RecordingSink(Mutex<Vec<(TenantId, Vec<Span>)>>);

    impl RecordingSink {
        fn batches(&self) -> Vec<(TenantId, Vec<Span>)> {
            self.0.lock().expect("sink lock").clone()
        }
    }

    impl SpanSink for RecordingSink {
        fn accept(
            &self,
            tenant: TenantId,
            spans: Vec<Span>,
        ) -> impl Future<Output = Result<(), SinkError>> + Send {
            self.0.lock().expect("sink lock").push((tenant, spans));
            async { Ok(()) }
        }
    }

    #[derive(Debug)]
    struct UnavailableSink;

    impl SpanSink for UnavailableSink {
        async fn accept(&self, _tenant: TenantId, _spans: Vec<Span>) -> Result<(), SinkError> {
            Err(SinkError::Unavailable)
        }
    }

    #[derive(Debug, Default)]
    struct RecordingMeter(Mutex<Vec<(TenantId, DecompressedBytes)>>);

    impl RecordingMeter {
        fn readings(&self) -> Vec<(TenantId, DecompressedBytes)> {
            self.0.lock().expect("meter lock").clone()
        }
    }

    impl UsageMeter for RecordingMeter {
        fn record_ingest(
            &self,
            tenant: TenantId,
            decompressed: DecompressedBytes,
        ) -> impl Future<Output = ()> + Send {
            self.0
                .lock()
                .expect("meter lock")
                .push((tenant, decompressed));
            async {}
        }
    }

    fn a_span() -> Span {
        Span {
            trace_id: TraceId::try_from([7u8; 16].as_slice()).expect("sixteen bytes"),
            span_id: SpanId::try_from([3u8; 8].as_slice()).expect("eight bytes"),
            parent: None,
            name: "GET /".into(),
            service_name: "checkout".into(),
            kind: SpanKind::Server,
            status: SpanStatus::Ok,
            start_unix_nanos: 1,
            end_unix_nanos: 2,
            attributes: BTreeMap::new(),
        }
    }

    fn a_payload(bytes: u64) -> Payload {
        Payload {
            spans: vec![a_span()],
            decompressed: DecompressedBytes::new(bytes),
        }
    }

    #[tokio::test]
    async fn an_unknown_key_reaches_no_sink() {
        let sink = RecordingSink::default();
        let meter = RecordingMeter::default();
        let ingest = Ingest::new(FakeDirectory::default(), Unmetered, &sink, &meter);

        let outcome = ingest
            .accept(&IngestKey::from_plaintext("absent"), a_payload(512))
            .await;

        assert_eq!(outcome, Err(IngestError::UnknownKey(UnknownKey)));
        assert!(sink.batches().is_empty());
        assert!(meter.readings().is_empty());
    }

    #[tokio::test]
    async fn a_known_key_sends_its_spans_under_its_tenant() {
        let tenant = TenantId::new(Uuid::from_u128(42));
        let sink = RecordingSink::default();
        let meter = RecordingMeter::default();
        let ingest = Ingest::new(
            FakeDirectory::holding("live", tenant),
            Unmetered,
            &sink,
            &meter,
        );

        let resolved = ingest
            .accept(&IngestKey::from_plaintext("live"), a_payload(512))
            .await
            .expect("the key resolves");

        assert_eq!(resolved, tenant);
        assert_eq!(sink.batches(), vec![(tenant, vec![a_span()])]);
    }

    #[tokio::test]
    async fn the_decompressed_size_is_metered_against_the_tenant() {
        let tenant = TenantId::new(Uuid::from_u128(7));
        let sink = RecordingSink::default();
        let meter = RecordingMeter::default();
        let ingest = Ingest::new(
            FakeDirectory::holding("live", tenant),
            Unmetered,
            &sink,
            &meter,
        );

        ingest
            .accept(&IngestKey::from_plaintext("live"), a_payload(4096))
            .await
            .expect("the key resolves");

        assert_eq!(
            meter.readings(),
            vec![(tenant, DecompressedBytes::new(4096))]
        );
    }

    #[tokio::test]
    async fn a_sink_failure_surfaces_to_the_caller() {
        let tenant = TenantId::new(Uuid::from_u128(1));
        let meter = RecordingMeter::default();
        let ingest = Ingest::new(
            FakeDirectory::holding("live", tenant),
            Unmetered,
            UnavailableSink,
            &meter,
        );

        let outcome = ingest
            .accept(&IngestKey::from_plaintext("live"), a_payload(1))
            .await;

        assert_eq!(outcome, Err(IngestError::Sink(SinkError::Unavailable)));
    }

    #[derive(Debug, Clone, Copy)]
    struct HardCap(u64);

    impl QuotaPolicy for HardCap {
        fn admit(
            &self,
            _tenant: TenantId,
            incoming: DecompressedBytes,
        ) -> impl Future<Output = Admission> + Send {
            let allowance = DecompressedBytes::new(self.0);
            let decision = if incoming > allowance {
                Admission::Capped { allowance }
            } else {
                Admission::Within
            };
            async move { decision }
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct AllowOverage(u64);

    impl QuotaPolicy for AllowOverage {
        fn admit(
            &self,
            _tenant: TenantId,
            incoming: DecompressedBytes,
        ) -> impl Future<Output = Admission> + Send {
            let over = incoming.get().saturating_sub(self.0);
            let decision = if over > 0 {
                Admission::Overage {
                    over_by: DecompressedBytes::new(over),
                }
            } else {
                Admission::Within
            };
            async move { decision }
        }
    }

    #[tokio::test]
    async fn a_capped_tenant_reaches_neither_the_meter_nor_the_sink() {
        let tenant = TenantId::new(Uuid::from_u128(4));
        let sink = RecordingSink::default();
        let meter = RecordingMeter::default();
        let ingest = Ingest::new(
            FakeDirectory::holding("live", tenant),
            HardCap(1024),
            &sink,
            &meter,
        );

        let outcome = ingest
            .accept(&IngestKey::from_plaintext("live"), a_payload(4096))
            .await;

        assert_eq!(
            outcome,
            Err(IngestError::Capped {
                allowance: DecompressedBytes::new(1024)
            })
        );
        assert!(sink.batches().is_empty());
        assert!(meter.readings().is_empty());
    }

    #[tokio::test]
    async fn an_allowed_overage_is_accepted_and_counted_in_full() {
        let tenant = TenantId::new(Uuid::from_u128(5));
        let sink = RecordingSink::default();
        let meter = RecordingMeter::default();
        let ingest = Ingest::new(
            FakeDirectory::holding("live", tenant),
            AllowOverage(1024),
            &sink,
            &meter,
        );

        let resolved = ingest
            .accept(&IngestKey::from_plaintext("live"), a_payload(4096))
            .await
            .expect("an overage is still accepted");

        assert_eq!(resolved, tenant);
        assert_eq!(sink.batches().len(), 1);
        assert_eq!(
            meter.readings(),
            vec![(tenant, DecompressedBytes::new(4096))],
            "the whole payload counts, not only the part inside the allowance"
        );
    }

    #[tokio::test]
    async fn a_payload_inside_the_allowance_passes_untouched() {
        let tenant = TenantId::new(Uuid::from_u128(6));
        let sink = RecordingSink::default();
        let meter = RecordingMeter::default();
        let ingest = Ingest::new(
            FakeDirectory::holding("live", tenant),
            HardCap(8192),
            &sink,
            &meter,
        );

        ingest
            .accept(&IngestKey::from_plaintext("live"), a_payload(4096))
            .await
            .expect("the payload fits");

        assert_eq!(sink.batches().len(), 1);
        assert_eq!(meter.readings().len(), 1);
    }
}
