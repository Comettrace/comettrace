mod config;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use clap::{Parser, Subcommand, ValueEnum};
use comettrace_api::{
    SessionCookie, SignInMethods, auth_router, catalogue_router, explore_router, health_router,
    me_router, methods_router, oidc_router, reset_router, router, trace_server, web_router,
};
use comettrace_core::{
    AccountId, AuthSources, Catalogue, Clock, DecompressedBytes, Email, Explore, FederatedSignIn,
    Ingest, IngestKey, Mailer, NoProvider, OrgRole, ProjectId, ProvisionPolicy, ResetError,
    ResetPassword, ResetStore, ResetToken, Scope, SessionLifetime, SignIn, SinkError, Span,
    SpanSink, TenantDirectory, TenantId, UnknownKey, UsageMeter, UsageReader, Whoami,
};
use comettrace_storage::{
    Allowance, Argon2Hasher, CONTROL_PLANE, ClickhouseReader, ClickhouseSink,
    LocalPasswordProvider, MailSettings, MemoryMeter, MemorySink, OidcProvider, OidcSettings,
    PostgresAccounts, PostgresDirectory, PostgresFlows, PostgresMemberships, PostgresProjects,
    PostgresResets, PostgresSessions, RedisMeter, SmtpMailer, SpanStoreSettings, StaticDirectory,
    StaticQuota, SystemClock, load_allowances, open_span_store,
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
    async fn accept(&self, scope: Scope, spans: Vec<Span>) -> Result<(), SinkError> {
        match self {
            Self::Memory(sink) => sink.accept(scope, spans).await,
            Self::Spans(sink) => sink.accept(scope, spans).await,
        }
    }
}

#[derive(Debug)]
enum Relay {
    Smtp(Box<SmtpMailer>),
    Unreachable,
}

impl Mailer for Relay {
    async fn send_password_reset(&self, to: &Email, token: &ResetToken) -> Result<(), ResetError> {
        match self {
            Self::Smtp(mailer) => mailer.send_password_reset(to, token).await,
            Self::Unreachable => {
                tracing::warn!("a reset link was asked for but no mail relay is configured");
                Err(ResetError::Undeliverable)
            }
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
    /// Ask the server on this host whether it is serving, for a healthcheck
    Health,
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
    /// Add a project to an organisation
    CreateProject {
        #[arg(long)]
        org: Uuid,
        #[arg(long)]
        name: String,
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

const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

async fn health(config: &Config) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{}/health", config.http_address.port());
    let answer = reqwest::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build()
        .context("preparing the request")?
        .get(&url)
        .send()
        .await
        .with_context(|| format!("asking {url}"))?;
    anyhow::ensure!(
        answer.status().is_success(),
        "{url} answered {}",
        answer.status()
    );
    Ok(())
}

async fn run_command(command: Command, config: &Config) -> anyhow::Result<()> {
    let pool = control_plane_pool(config)
        .await?
        .context("this command needs a control plane, so set database_url")?;

    match command {
        Command::Health => health(config).await,
        Command::IssueKey { org, project } => issue_key(pool, org, project).await,
        Command::RevokeKey { prefix } => revoke_key(pool, &prefix).await,
        Command::Bootstrap { email, org } => {
            bootstrap(pool, &email, &org, config.local_password).await
        }
        Command::CreateAccount { email } => create_account(pool, &email).await,
        Command::ResetPassword { email } => reset_password(pool, &email).await,
        Command::CreateProject { org, name } => create_project(pool, org, &name).await,
        Command::CreateOrg { name, owner } => create_org(pool, &name, &owner).await,
        Command::GrantRole { org, member, role } => {
            grant_role(pool, org, &member, role.into()).await
        }
    }
}

/* A span carries the project of the key that brought it, so a key with no project writes spans no
project filter can match. When the organisation holds exactly one there is nothing to choose
between, so the key gets it; with several the operator has to say which. */
async fn issue_key(pool: PgPool, org: Uuid, project: Option<Uuid>) -> anyhow::Result<()> {
    let tenant = TenantId::new(org);
    let project = match project {
        Some(named) => Some(named),
        None => PostgresProjects::new(pool.clone())
            .only(tenant)
            .await
            .context("looking for the only project")?
            .map(ProjectId::as_uuid),
    };
    if project.is_none() {
        eprintln!(
            "This organisation holds no single project, so this key names none \
             and its spans will match no project. Name one with --project."
        );
    }
    let key = PostgresDirectory::new(pool, SystemClock)
        .issue(tenant, project)
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

async fn bootstrap(
    pool: PgPool,
    email: &str,
    org: &str,
    local_password: bool,
) -> anyhow::Result<()> {
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
    let project = PostgresProjects::new(pool.clone())
        .create(tenant, "Default")
        .await
        .context("creating the first project")?;

    eprintln!("{} owns {org} ({tenant}).", email.as_str());
    eprintln!("It holds one project, Default ({project}).");
    eprintln!("Draw its first ingest key with:");
    eprintln!("  comettrace issue-key --org {tenant} --project {project}");

    if !local_password {
        eprintln!("Local passwords are off, so sign in through the identity provider.");
        return Ok(());
    }
    print_the_way_in(&invite(&pool, account).await?);
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

async fn create_project(pool: PgPool, org: Uuid, name: &str) -> anyhow::Result<()> {
    let project = PostgresProjects::new(pool)
        .create(TenantId::new(org), name)
        .await
        .context("creating the project")?;
    eprintln!("{org} now holds {name}. Draw a key for it with:");
    eprintln!("  comettrace issue-key --org {org} --project {project}");
    println!("{project}");
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
        .with_writer(std::io::stderr)
        .init();

    let config = config::load().context("reading the configuration")?;
    match Cli::parse().command {
        Some(Command::Health) => return health(&config).await,
        Some(command) => return run_command(command, &config).await,
        None => {}
    }
    let pool = control_plane_pool(&config).await?;
    let allowances = allowances_of(&config, pool.as_ref()).await?;
    let meter = Arc::new(meter_of(&config, pool.as_ref()).await?);
    if let (Some(pool), Meter::Redis(_)) = (pool.as_ref(), meter.as_ref()) {
        tokio::spawn(roll_up_hourly(Arc::clone(&meter), pool.clone()));
    }
    let (sink, reader) = spans_of(&config).await?;
    let sink = Arc::new(sink);
    tokio::spawn(flush_spans(Arc::clone(&sink)));
    let identity = identity_routes(&config, pool.as_ref())
        .await?
        .merge(explore_routes(pool.as_ref(), reader))
        .merge(catalogue_routes(pool.as_ref()));
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
        web_routes(&config),
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
    async fn resolve(&self, key: &IngestKey) -> Result<Scope, UnknownKey> {
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

type Looking = Explore<Arc<Sessions>, PostgresMemberships, ClickhouseReader>;
type Holding = Catalogue<Arc<Sessions>, PostgresMemberships, PostgresProjects>;

fn catalogue_routes(pool: Option<&PgPool>) -> Router {
    let Some(pool) = pool else {
        return Router::new();
    };
    let sessions = Arc::new(PostgresSessions::new(
        pool.clone(),
        SystemClock,
        SessionLifetime::default(),
    ));
    let catalogue: Arc<Holding> = Arc::new(Catalogue::new(
        Whoami::new(sessions, PostgresMemberships::new(pool.clone())),
        PostgresProjects::new(pool.clone()),
    ));
    catalogue_router(catalogue)
}

fn explore_routes(pool: Option<&PgPool>, reader: Option<ClickhouseReader>) -> Router {
    let (Some(pool), Some(reader)) = (pool, reader) else {
        tracing::warn!(
            "traces cannot be read back: that needs both a control plane to \
             authorise the reader and a span store to read from"
        );
        return Router::new();
    };
    let sessions = Arc::new(PostgresSessions::new(
        pool.clone(),
        SystemClock,
        SessionLifetime::default(),
    ));
    let explore: Arc<Looking> = Arc::new(Explore::new(
        Whoami::new(sessions, PostgresMemberships::new(pool.clone())),
        reader,
    ));
    explore_router(explore)
}

async fn spans_of(config: &Config) -> anyhow::Result<(Sink, Option<ClickhouseReader>)> {
    let Some(spans) = config.spans.clone() else {
        tracing::warn!(
            "no span store is configured, so accepted spans are held in memory and \
             lost when this process ends, and no trace can be read back; \
             set spans.url to keep them"
        );
        return Ok((Sink::Memory(MemorySink::default()), None));
    };
    let settings = SpanStoreSettings::from(spans);
    let store = open_span_store(&settings, SystemClock)
        .await
        .context("opening the span store")?;
    tracing::info!(
        url = %settings.url,
        retention_days = settings.retention_days,
        "spans are written to the span store"
    );
    Ok((Sink::Spans(Box::new(store.sink)), Some(store.reader)))
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
type Resets = ResetPassword<PostgresResets<SystemClock>, Argon2Hasher, Relay>;
type Me = Whoami<Arc<Sessions>, PostgresMemberships>;
type Federated =
    FederatedSignIn<OidcProvider, PostgresFlows<SystemClock>, PostgresAccounts, Arc<Sessions>>;

async fn identity_routes(config: &Config, pool: Option<&PgPool>) -> anyhow::Result<Router> {
    let offered = methods_router(SignInMethods {
        local_password: config.local_password && pool.is_some(),
        oidc: config.oidc.is_some() && pool.is_some(),
    });
    let Some(pool) = pool else {
        if config.local_password || config.oidc.is_some() {
            tracing::warn!("no control plane is configured, so nobody can sign in");
        }
        return Ok(offered);
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
    let mut routes = offered.merge(me_router(whoami));

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

    if config.mail.is_some() && !config.local_password {
        tracing::warn!("a mail relay is configured but local passwords are off");
    }

    if config.local_password {
        let hasher = Argon2Hasher::new().context("preparing the password hasher")?;
        let reset: Arc<Resets> = Arc::new(ResetPassword::new(
            PostgresResets::new(pool.clone(), SystemClock),
            hasher,
            relay_of(config)?,
        ));
        routes = routes.merge(reset_router(reset));
    }

    Ok(routes)
}

fn relay_of(config: &Config) -> anyhow::Result<Relay> {
    let Some(mail) = &config.mail else {
        tracing::warn!(
            "no mail relay is configured, so nobody can ask for a reset link; \
             hand one over with `comettrace reset-password`"
        );
        return Ok(Relay::Unreachable);
    };
    let mailer = SmtpMailer::connect(&MailSettings {
        relay: mail.relay.clone(),
        username: mail.username.clone(),
        password: mail.password.clone(),
        from: mail.from.clone(),
        base_url: mail.base_url.clone(),
    })
    .context("preparing the mail relay")?;
    tracing::info!(relay = %mail.relay, "password resets can be sent");
    Ok(Relay::Smtp(Box::new(mailer)))
}

async fn serve_http(
    address: SocketAddr,
    gateway: Arc<Gateway>,
    identity: Router,
    web: Router,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("binding the OTLP HTTP listener on {address}"))?;
    let served = router(gateway)
        .merge(identity)
        .merge(health_router())
        .merge(web);
    axum::serve(listener, served)
        .with_graceful_shutdown(shutdown())
        .await
        .context("serving OTLP over HTTP")
}

fn web_routes(config: &Config) -> Router {
    let Some(root) = config.web_root.as_deref().map(Path::new) else {
        return Router::new();
    };
    if !root.join("index.html").is_file() {
        tracing::warn!(
            root = %root.display(),
            "web_root carries no index.html, so nothing is served at the root"
        );
        return Router::new();
    }
    tracing::info!(root = %root.display(), "the web application is served");
    web_router(root)
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
