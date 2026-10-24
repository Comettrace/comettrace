mod config;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use comettrace_api::{router, trace_server};
use comettrace_core::{Clock, DecompressedBytes, Ingest, TenantId, UsageMeter, UsageReader};
use comettrace_storage::{
    Allowance, MemoryMeter, MemorySink, RedisMeter, StaticDirectory, StaticQuota, SystemClock,
    load_allowances,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

const SECONDS_PER_HOUR: u64 = 3600;

type Gateway = Ingest<StaticDirectory, StaticQuota<Arc<Meter>>, Arc<MemorySink>, Arc<Meter>>;

#[derive(Debug)]
enum Meter {
    Memory(MemoryMeter),
    Redis(RedisMeter<SystemClock>),
}

impl UsageMeter for Meter {
    async fn record_ingest(&self, tenant: TenantId, decompressed: DecompressedBytes) {
        match self {
            Self::Memory(meter) => meter.record_ingest(tenant, decompressed).await,
            Self::Redis(meter) => meter.record_ingest(tenant, decompressed).await,
        }
    }
}

impl UsageReader for Meter {
    async fn used(&self, tenant: TenantId) -> DecompressedBytes {
        match self {
            Self::Memory(meter) => meter.used(tenant).await,
            Self::Redis(meter) => meter.used(tenant).await,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::load().context("reading the configuration")?;
    let pool = control_plane_pool(&config).await?;
    let allowances = allowances_of(&config, pool.as_ref()).await?;
    let meter = Arc::new(meter_of(&config, pool.as_ref()).await?);
    match (pool, meter.as_ref()) {
        (Some(pool), Meter::Redis(_)) => {
            tokio::spawn(roll_up_hourly(Arc::clone(&meter), pool));
        }
        (Some(pool), Meter::Memory(_)) => pool.close().await,
        (None, _) => {}
    }
    let gateway = Arc::new(gateway_of(&config, allowances, meter));

    tracing::info!(
        grpc = %config.grpc_address,
        http = %config.http_address,
        tenants = config.tenants.len(),
        "the ingestion gateway is starting"
    );

    let http = tokio::spawn(serve_http(config.http_address, Arc::clone(&gateway)));
    let grpc = tokio::spawn(serve_grpc(config.grpc_address, gateway));

    let (http, grpc) = tokio::try_join!(http, grpc).context("joining the listeners")?;
    http?;
    grpc?;
    Ok(())
}

async fn control_plane_pool(config: &Config) -> anyhow::Result<Option<PgPool>> {
    let Some(url) = config.database_url.as_deref() else {
        return Ok(None);
    };
    PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .map(Some)
        .context("connecting to the control plane")
}

async fn allowances_of(
    config: &Config,
    pool: Option<&PgPool>,
) -> anyhow::Result<BTreeMap<TenantId, Allowance>> {
    let Some(pool) = pool else {
        return Ok(config
            .tenants
            .iter()
            .map(|tenant| (TenantId::new(tenant.id), tenant.allowance.into()))
            .collect());
    };
    let loaded = load_allowances(pool)
        .await
        .context("reading the tenant allowances")?;
    tracing::info!(
        tenants = loaded.len(),
        "allowances read from the control plane"
    );
    Ok(loaded)
}

async fn meter_of(config: &Config, pool: Option<&PgPool>) -> anyhow::Result<Meter> {
    let Some(url) = config.redis_url.as_deref() else {
        return Ok(Meter::Memory(MemoryMeter::default()));
    };

    let meter = RedisMeter::connect(url, SystemClock)
        .await
        .context("connecting to the usage counter store")?;
    if let Some(pool) = pool {
        let seeded = meter
            .rehydrate(pool)
            .await
            .context("rehydrating the usage counters")?;
        tracing::info!(tenants = seeded, "usage counters rehydrated");
    }
    Ok(Meter::Redis(meter))
}

fn gateway_of(
    config: &Config,
    allowances: BTreeMap<TenantId, Allowance>,
    meter: Arc<Meter>,
) -> Gateway {
    let directory = StaticDirectory::from_plaintext(
        config
            .tenants
            .iter()
            .map(|tenant| (tenant.key.clone(), TenantId::new(tenant.id))),
    );
    let quota = StaticQuota::new(Arc::clone(&meter), allowances);
    Ingest::new(directory, quota, Arc::new(MemorySink::default()), meter)
}

async fn serve_http(address: SocketAddr, gateway: Arc<Gateway>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("binding the OTLP HTTP listener on {address}"))?;
    axum::serve(listener, router(gateway))
        .with_graceful_shutdown(shutdown())
        .await
        .context("serving OTLP over HTTP")
}

async fn serve_grpc(address: SocketAddr, gateway: Arc<Gateway>) -> anyhow::Result<()> {
    Server::builder()
        .add_service(trace_server(gateway))
        .serve_with_shutdown(address, shutdown())
        .await
        .context("serving OTLP over gRPC")
}

async fn shutdown() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "the shutdown signal handler failed");
    }
}

async fn roll_up_hourly(meter: Arc<Meter>, pool: PgPool) {
    let Meter::Redis(redis) = meter.as_ref() else {
        return;
    };
    loop {
        tokio::time::sleep(until_the_next_hour(&SystemClock)).await;
        match redis.roll_up(&pool).await {
            Ok(tenants) => tracing::info!(tenants, "the finished hour was rolled up"),
            Err(error) => tracing::error!(%error, "the usage rollup failed"),
        }
    }
}

fn until_the_next_hour(clock: &SystemClock) -> Duration {
    let elapsed = clock.now().get() % SECONDS_PER_HOUR;
    Duration::from_secs(SECONDS_PER_HOUR - elapsed)
}
