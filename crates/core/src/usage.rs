use core::future::Future;

use crate::tenant::TenantId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct DecompressedBytes(u64);

impl DecompressedBytes {
    pub fn new(count: u64) -> Self {
        Self(count)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

pub trait UsageMeter {
    fn record_ingest(
        &self,
        tenant: TenantId,
        decompressed: DecompressedBytes,
    ) -> impl Future<Output = ()> + Send;
}

impl<T: UsageMeter + Sync> UsageMeter for &T {
    fn record_ingest(
        &self,
        tenant: TenantId,
        decompressed: DecompressedBytes,
    ) -> impl Future<Output = ()> + Send {
        (*self).record_ingest(tenant, decompressed)
    }
}

impl<T: UsageMeter + Send + Sync> UsageMeter for std::sync::Arc<T> {
    fn record_ingest(
        &self,
        tenant: TenantId,
        decompressed: DecompressedBytes,
    ) -> impl Future<Output = ()> + Send {
        self.as_ref().record_ingest(tenant, decompressed)
    }
}
