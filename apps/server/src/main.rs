mod config;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use clap::{Parser, Subcommand};
use comettrace_api::{SessionCookie, auth_router, oidc_router, router, trace_server};
use comettrace_core::{
    AuthSources, Clock, DecompressedBytes, FederatedSignIn, Ingest, IngestKey, NoProvider,
    ProvisionPolicy, SessionLifetime, SignIn, TenantDirectory, TenantId, UnknownKey, UsageMeter,
    UsageReader,
};
use comettrace_storage::{
    Allowance, Argon2Hasher, LocalPasswordProvider, MemoryMeter, MemorySink, OidcProvider,
    OidcSettings, PostgresAccounts, PostgresDirectory, PostgresFlows, PostgresSessions, RedisMeter,
    StaticDirectory, StaticQuota, SystemClock, load_allowances,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::config::Config;

const SECONDS_PER_HOUR: u64 = 3600;

type Gateway = Ingest<Directory, StaticQuota<Arc<Meter>>, Arc<MemorySink>, Arc<Meter>>;

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

#[derive(Debug, Parser)]
#[command(name = "comettrace", about = "OTLP ingestion and its control plane")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Draw a new ingest key for an organisation and print it once
    IssueKey {
        #[arg(long)]
        org: Uuid,
        #[arg(long)]
        project: Option<Uuid>,
    },
    /// Stop a key from authenticating, named by its clear prefix
    RevokeKey {
        #[arg(long)]
        prefix: String,
    },
}

async fn run_command(command: Command, config: &Config) -> anyhow::Result<()> {
    let pool = control_plane_pool(config)
        .await?
        .context("this command needs a control plane, so set database_url")?;
    let directory = PostgresDirectory::new(pool, SystemClock);

    match command {
        Command::IssueKey { org, project } => {
            let key = directory
                .issue(TenantId::new(org), project)
                .await
                .context("issuing the key")?;
            eprintln!("This key is shown once. Store it now; it cannot be read back.");
            println!("{}", key.exposed());
        }
        Command::RevokeKey { prefix } => {
            let stopped = directory
                .revoke(&prefix)
                .await
                .context("revoking the key")?;
            if stopped {
                eprintln!("{prefix} will not authenticate the next payload.");
            } else {
                eprintln!("{prefix} matched no live key.");
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::load().context("reading the configuration")?;
    if let Some(command) = Cli::parse().command {
        return run_command(command, &config).await;
    }
    let pool = control_plane_pool(&config).await?;
    let allowances = allowances_of(&config, pool.as_ref()).await?;
    let meter = Arc::new(meter_of(&config, pool.as_ref()).await?);
    let identity = identity_routes(&config, pool.as_ref()).await?;
    if let (Some(pool), Meter::Redis(_)) = (pool.as_ref(), meter.as_ref()) {
        tokio::spawn(roll_up_hourly(Arc::clone(&meter), pool.clone()));
    }
    let gateway = Arc::new(gateway_of(directory_of(pool.as_ref()), allowances, meter));

    tracing::info!(
        grpc = %config.grpc_address,
        http = %config.http_address,
        tenants = config.tenants.len(),
        "the ingestion gateway is starting"
    );

    let http = tokio::spawn(serve_http(
        config.http_address,
        Arc::clone(&gateway),
        identity,
    ));
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

#[derive(Debug)]
enum Directory {
    Nothing(StaticDirectory),
    ControlPlane(PostgresDirectory<SystemClock>),
}

impl TenantDirectory for Directory {
    async fn resolve(&self, key: &IngestKey) -> Result<TenantId, UnknownKey> {
        match self {
            Self::Nothing(directory) => directory.resolve(key).await,
            Self::ControlPlane(directory) => directory.resolve(key).await,
        }
    }
}

fn directory_of(pool: Option<&PgPool>) -> Directory {
    let Some(pool) = pool else {
        tracing::warn!(
            "no control plane is configured, so no ingest key can authenticate; \
             issue one with `comettrace issue-key` once database_url is set"
        );
        return Directory::Nothing(StaticDirectory::default());
    };
    Directory::ControlPlane(PostgresDirectory::new(pool.clone(), SystemClock))
}

fn gateway_of(
    directory: Directory,
    allowances: BTreeMap<TenantId, Allowance>,
    meter: Arc<Meter>,
) -> Gateway {
    let quota = StaticQuota::new(Arc::clone(&meter), allowances);
    Ingest::new(directory, quota, Arc::new(MemorySink::default()), meter)
}

type LocalSource = LocalPasswordProvider<Argon2Hasher>;
type Sessions = PostgresSessions<SystemClock>;
type PasswordSignIn = SignIn<LocalSource, NoProvider, PostgresAccounts, Arc<Sessions>>;
type Federated =
    FederatedSignIn<OidcProvider, PostgresFlows<SystemClock>, PostgresAccounts, Arc<Sessions>>;

async fn identity_routes(config: &Config, pool: Option<&PgPool>) -> anyhow::Result<Router> {
    let Some(pool) = pool else {
        if config.local_password || config.oidc.is_some() {
            tracing::warn!("no control plane is configured, so nobody can sign in");
        }
        return Ok(Router::new());
    };

    let cookie = SessionCookie {
        secure: config.cookie_secure,
        ..SessionCookie::default()
    };
    let accounts = PostgresAccounts::new(pool.clone());
    let sessions = Arc::new(PostgresSessions::new(
        pool.clone(),
        SystemClock,
        SessionLifetime::default(),
    ));
    let mut routes = Router::new();

    if config.local_password {
        let hasher = Argon2Hasher::new().context("preparing the password hasher")?;
        let sign_in: Arc<PasswordSignIn> = Arc::new(SignIn::new(
            AuthSources::new(Some(LocalPasswordProvider::new(pool.clone(), hasher)), None),
            accounts.clone(),
            Arc::clone(&sessions),
            ProvisionPolicy::default(),
        ));
        routes = routes.merge(auth_router(sign_in, cookie));
        tracing::info!("local passwords accepted");
    }

    if let Some(settings) = &config.oidc {
        let provider = OidcProvider::discover(OidcSettings {
            issuer: settings.issuer.clone(),
            client_id: settings.client_id.clone(),
            client_secret: settings.client_secret.clone(),
            redirect_uri: settings.redirect_uri.clone(),
        })
        .await
        .context("reaching the identity provider")?;
        let federated: Arc<Federated> = Arc::new(FederatedSignIn::new(
            provider,
            PostgresFlows::new(pool.clone(), SystemClock),
            accounts,
            sessions,
            ProvisionPolicy::default(),
        ));
        routes = routes.merge(oidc_router(federated, cookie, config.after_sign_in.clone()));
        tracing::info!(issuer = %settings.issuer, "federated sign in accepted");
    }

    Ok(routes)
}

async fn serve_http(
    address: SocketAddr,
    gateway: Arc<Gateway>,
    identity: Router,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("binding the OTLP HTTP listener on {address}"))?;
    axum::serve(listener, router(gateway).merge(identity))
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
        tokio::time::sleep(until_the_next_hour(SystemClock)).await;
        match redis.roll_up(&pool).await {
            Ok(tenants) => tracing::info!(tenants, "the finished hour was rolled up"),
            Err(error) => tracing::error!(%error, "the usage rollup failed"),
        }
    }
}

fn until_the_next_hour(clock: SystemClock) -> Duration {
    let elapsed = clock.now().get() % SECONDS_PER_HOUR;
    Duration::from_secs(SECONDS_PER_HOUR - elapsed)
}
