mod config;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use comettrace_api::{router, trace_server};
use comettrace_core::{Ingest, TenantId};
use comettrace_storage::{MemoryMeter, MemorySink, StaticDirectory, StaticQuota};
use tokio::net::TcpListener;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

type Gateway =
    Ingest<StaticDirectory, StaticQuota<Arc<MemoryMeter>>, Arc<MemorySink>, Arc<MemoryMeter>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::load().context("reading the configuration")?;
    let gateway = Arc::new(gateway_of(&config));

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

fn gateway_of(config: &Config) -> Gateway {
    let directory = StaticDirectory::from_plaintext(
        config
            .tenants
            .iter()
            .map(|tenant| (tenant.key.clone(), TenantId::new(tenant.id))),
    );
    let meter = Arc::new(MemoryMeter::default());
    let quota = StaticQuota::new(
        Arc::clone(&meter),
        config
            .tenants
            .iter()
            .map(|tenant| (TenantId::new(tenant.id), tenant.allowance.into())),
    );
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
