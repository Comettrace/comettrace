mod config;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use clap::{Parser, Subcommand, ValueEnum};
use comettrace_api::{
    SessionCookie, auth_router, health_router, me_router, oidc_router, reset_router, router,
    trace_server,
};
use comettrace_core::{
    AccountId, AuthSources, Clock, DecompressedBytes, Email, FederatedSignIn, Ingest, IngestKey,
    NoProvider, OrgRole, ProvisionPolicy, ResetPassword, ResetStore, ResetToken, SessionLifetime,
    SignIn, SinkError, Span, SpanSink, TenantDirectory, TenantId, UnknownKey, UsageMeter,
    UsageReader, Whoami,
};
use comettrace_storage::{
    Allowance, Argon2Hasher, CONTROL_PLANE, ClickhouseSink, LocalPasswordProvider, MailSettings,
    MemoryMeter, MemorySink, OidcProvider, OidcSettings, PostgresAccounts, PostgresDirectory,
    PostgresFlows, PostgresMemberships, PostgresResets, PostgresSessions, RedisMeter, SmtpMailer,
    SpanStoreSettings, StaticDirectory, StaticQuota, SystemClock, load_allowances, open_span_store,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::config::Config;

const SECONDS_PER_HOUR: u64 = 3600;
const FLUSH_FALLBACK: Duration = Duration::from_secs(5);
const MIN_FLUSH_WAIT: Duration = Duration::from_millis(100);

type Gateway = Ingest<Directory, StaticQuota<Arc<Meter>>, Arc<Sink>, Arc<Meter>>;

#[derive(Debug)]
enum Sink {
    Memory(MemorySink),
    Spans(Box<ClickhouseSink<SystemClock>>),
}

impl SpanSink for Sink {
    async fn accept(&self, tenant: TenantId, spans: Vec<Span>) -> Result<(), SinkError> {
        match self {
            Self::Memory(sink) => sink.accept(tenant, spans).await,
            Self::Spans(sink) => sink.accept(tenant, spans).await,
        }
    }
}

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
    /// Give a fresh deployment one account, one organisation, and the way in
    Bootstrap {
        #[arg(long)]
        email: String,
        #[arg(long, default_value = "Default")]
        org: String,
    },
    /// Create an account with no password and print the link that sets one
    CreateAccount {
        #[arg(long)]
        email: String,
    },
    /// Print a fresh link that sets the password of an existing account
    ResetPassword {
        #[arg(long)]
        email: String,
    },
    /// Create an organisation and make an existing account its owner
    CreateOrg {
        #[arg(long)]
        name: String,
        #[arg(long)]
        owner: String,
    },
    /// Give an existing account a role in an organisation
    GrantRole {
        #[arg(long)]
        org: Uuid,
        #[arg(long)]
        member: String,
        #[arg(long)]
        role: Role,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Role {
    Owner,
    Admin,
    Member,
}

impl From<Role> for OrgRole {
    fn from(role: Role) -> Self {
        match role {
            Role::Owner => Self::Owner,
            Role::Admin => Self::Admin,
            Role::Member => Self::Member,
        }
    }
}

const INVITE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

async fn invite(pool: &PgPool, account: AccountId) -> anyhow::Result<ResetToken> {
    let token = ResetToken::random();
    PostgresResets::new(pool.clone(), SystemClock)
        .with_window(INVITE_WINDOW)
        .begin(account, token.hash())
        .await
        .context("opening the password reset")?;
    Ok(token)
}

fn print_the_way_in(token: &ResetToken) {
    eprintln!(
        "The token printed below lasts {} hours and can be spent once:",
        INVITE_WINDOW.as_secs() / 3600
    );
    eprintln!(
        "  curl -X POST <base>/auth/reset/confirm -H 'x-requested-with: cli' \\\n    \
         -H 'content-type: application/json' \\\n    \
         -d '{{\"token\":\"<token>\",\"password\":\"<chosen>\"}}'"
    );
    println!("{}", token.exposed());
}

async fn run_command(command: Command, config: &Config) -> anyhow::Result<()> {
    let pool = control_plane_pool(config)
        .await?
        .context("this command needs a control plane, so set database_url")?;

    match command {
        Command::IssueKey { org, project } => issue_key(pool, org, project).await,
        Command::RevokeKey { prefix } => revoke_key(pool, &prefix).await,
        Command::Bootstrap { email, org } => bootstrap(pool, &email, &org).await,
        Command::CreateAccount { email } => create_account(pool, &email).await,
        Command::ResetPassword { email } => reset_password(pool, &email).await,
        Command::CreateOrg { name, owner } => create_org(pool, &name, &owner).await,
        Command::GrantRole { org, member, role } => {
            grant_role(pool, org, &member, role.into()).await
        }
    }
}

async fn issue_key(pool: PgPool, org: Uuid, project: Option<Uuid>) -> anyhow::Result<()> {
    let key = PostgresDirectory::new(pool, SystemClock)
        .issue(TenantId::new(org), project)
        .await
        .context("issuing the key")?;
    eprintln!("This key is shown once. Store it now; it cannot be read back.");
    println!("{}", key.exposed());
    Ok(())
}

async fn revoke_key(pool: PgPool, prefix: &str) -> anyhow::Result<()> {
    let stopped = PostgresDirectory::new(pool, SystemClock)
        .revoke(prefix)
        .await
        .context("revoking the key")?;
    if stopped {
        eprintln!("{prefix} will not authenticate the next payload.");
    } else {
        eprintln!("{prefix} matched no live key.");
    }
    Ok(())
}

async fn bootstrap(pool: PgPool, email: &str, org: &str) -> anyhow::Result<()> {
    let memberships = PostgresMemberships::new(pool.clone());
    if let Some(existing) = memberships
        .founded()
        .await
        .context("looking for an organisation")?
    {
        eprintln!(
            "This deployment already holds an organisation ({existing}), \
             so nothing was changed."
        );
        return Ok(());
    }

    let email = Email::try_from(email).context("reading the address")?;
    let account = account_for_or_create(&pool, &email).await?;
    let tenant = memberships
        .found(org, email.as_str())
        .await
        .context("creating the organisation")?;
    let token = invite(&pool, account).await?;

    eprintln!("{} owns {org} ({tenant}).", email.as_str());
    eprintln!("Draw its first ingest key with:");
    eprintln!("  comettrace issue-key --org {tenant}");
    print_the_way_in(&token);
    Ok(())
}

async fn account_for_or_create(pool: &PgPool, email: &Email) -> anyhow::Result<AccountId> {
    let found = PostgresResets::new(pool.clone(), SystemClock)
        .account_for(email)
        .await
        .context("looking the account up")?;
    if let Some(account) = found {
        return Ok(account);
    }
    create_locked_account(pool, email).await
}

async fn create_locked_account(pool: &PgPool, email: &Email) -> anyhow::Result<AccountId> {
    let hasher = Argon2Hasher::new().context("preparing the password hasher")?;
    let locked = hasher.locked().context("locking the new account")?;
    PostgresAccounts::new(pool.clone())
        .create(email, &locked)
        .await
        .context("creating the account")
}

async fn create_account(pool: PgPool, email: &str) -> anyhow::Result<()> {
    let email = Email::try_from(email).context("reading the address")?;
    let account = create_locked_account(&pool, &email).await?;
    let token = invite(&pool, account).await?;
    eprintln!(
        "{} cannot sign in until it carries a password.",
        email.as_str()
    );
    print_the_way_in(&token);
    Ok(())
}

async fn reset_password(pool: PgPool, email: &str) -> anyhow::Result<()> {
    let email = Email::try_from(email).context("reading the address")?;
    let account = PostgresResets::new(pool.clone(), SystemClock)
        .account_for(&email)
        .await
        .context("looking the account up")?
        .with_context(|| format!("no account carries {}", email.as_str()))?;
    let token = invite(&pool, account).await?;
    eprintln!(
        "Every session of {} ends once the password changes.",
        email.as_str()
    );
    print_the_way_in(&token);
    Ok(())
}

async fn create_org(pool: PgPool, name: &str, owner: &str) -> anyhow::Result<()> {
    let org = PostgresMemberships::new(pool)
        .found(name, owner)
        .await
        .context("creating the organisation")?;
    eprintln!("{owner} now owns {name}. Issue its first key with:");
    eprintln!("  comettrace issue-key --org {org}");
    println!("{org}");
    Ok(())
}

async fn grant_role(pool: PgPool, org: Uuid, member: &str, role: OrgRole) -> anyhow::Result<()> {
    PostgresMemberships::new(pool)
        .grant(TenantId::new(org), member, role)
        .await
        .context("granting the role")?;
    eprintln!("{member} is now {role} of {org}.");
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
    let sink = Arc::new(sink_of(&config).await?);
    tokio::spawn(flush_spans(Arc::clone(&sink)));
    let gateway = Arc::new(gateway_of(
        directory_of(pool.as_ref()),
        allowances,
        meter,
        Arc::clone(&sink),
    ));

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
    flush_once(&sink).await;
    http?;
    grpc?;
    Ok(())
}

async fn control_plane_pool(config: &Config) -> anyhow::Result<Option<PgPool>> {
    let Some(url) = config.database_url.as_deref() else {
        return Ok(None);
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .context("connecting to the control plane")?;
    CONTROL_PLANE
        .run(&pool)
        .await
        .context("migrating the control plane")?;
    Ok(Some(pool))
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
    sink: Arc<Sink>,
) -> Gateway {
    let quota = StaticQuota::new(Arc::clone(&meter), allowances);
    Ingest::new(directory, quota, sink, meter)
}

async fn sink_of(config: &Config) -> anyhow::Result<Sink> {
    let Some(spans) = config.spans.clone() else {
        tracing::warn!(
            "no span store is configured, so accepted spans are held in memory and \
             lost when this process ends; set spans.url to keep them"
        );
        return Ok(Sink::Memory(MemorySink::default()));
    };
    let settings = SpanStoreSettings::from(spans);
    let sink = open_span_store(&settings, SystemClock)
        .await
        .context("opening the span store")?;
    tracing::info!(
        url = %settings.url,
        retention_days = settings.retention_days,
        "spans are written to the span store"
    );
    Ok(Sink::Spans(Box::new(sink)))
}

async fn flush_spans(sink: Arc<Sink>) {
    let Sink::Spans(spans) = sink.as_ref() else {
        return;
    };
    loop {
        let wait = spans
            .time_left()
            .await
            .unwrap_or(FLUSH_FALLBACK)
            .max(MIN_FLUSH_WAIT);
        tokio::time::sleep(wait).await;
        match spans.flush().await {
            Ok(0) => {}
            Ok(rows) => tracing::debug!(rows, "a span batch was committed on its deadline"),
            Err(error) => tracing::error!(%error, "a span batch could not be committed"),
        }
    }
}

async fn flush_once(sink: &Sink) {
    let Sink::Spans(spans) = sink else {
        return;
    };
    match spans.flush().await {
        Ok(0) => {}
        Ok(rows) => tracing::info!(rows, "the last span batch was committed before leaving"),
        Err(error) => tracing::error!(%error, "the last span batch was lost"),
    }
}

type LocalSource = LocalPasswordProvider<Argon2Hasher>;
type Sessions = PostgresSessions<SystemClock>;
type PasswordSignIn = SignIn<LocalSource, NoProvider, PostgresAccounts, Arc<Sessions>>;
type Resets = ResetPassword<PostgresResets<SystemClock>, Argon2Hasher, SmtpMailer>;
type Me = Whoami<Arc<Sessions>, PostgresMemberships>;
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
    let whoami: Arc<Me> = Arc::new(Whoami::new(
        Arc::clone(&sessions),
        PostgresMemberships::new(pool.clone()),
    ));
    let mut routes = me_router(whoami);

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

    if let Some(mail) = &config.mail {
        if !config.local_password {
            tracing::warn!("a mail relay is configured but local passwords are off");
        }
        let mailer = SmtpMailer::connect(&MailSettings {
            relay: mail.relay.clone(),
            username: mail.username.clone(),
            password: mail.password.clone(),
            from: mail.from.clone(),
            base_url: mail.base_url.clone(),
        })
        .context("preparing the mail relay")?;
        let hasher = Argon2Hasher::new().context("preparing the password hasher")?;
        let reset: Arc<Resets> = Arc::new(ResetPassword::new(
            PostgresResets::new(pool.clone(), SystemClock),
            hasher,
            mailer,
        ));
        routes = routes.merge(reset_router(reset));
        tracing::info!(relay = %mail.relay, "password resets can be sent");
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
    let served = router(gateway).merge(identity).merge(health_router());
    axum::serve(listener, served)
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
