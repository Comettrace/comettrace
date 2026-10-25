use std::net::SocketAddr;

use comettrace_core::DecompressedBytes;
use comettrace_storage::{Allowance, SpanStoreSettings};
use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const FILE: &str = "comettrace.toml";
const PREFIX: &str = "COMETTRACE_";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub grpc_address: SocketAddr,
    pub http_address: SocketAddr,
    #[serde(default)]
    pub database_url: Option<String>,
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default)]
    pub spans: Option<SpansConfig>,
    #[serde(default)]
    pub web_root: Option<String>,
    #[serde(default)]
    pub tenants: Vec<TenantKey>,
    #[serde(default)]
    pub oidc: Option<OidcConfig>,
    #[serde(default)]
    pub mail: Option<MailConfig>,
    /* Thirty-two bytes of hex, which is what a token for somebody else's service is sealed
    under. A deployment that sets none can do everything it did before; what it cannot do is hold
    such a token, and the integrations screen says so rather than the server refusing to start.
    Lose it and the sealed rows are unreadable — which is the whole bargain, and why
    `comettrace secret-key` prints one rather than anybody inventing their own. */
    #[serde(default)]
    pub secret_key: Option<String>,
    #[serde(default)]
    pub linear: Option<LinearConfig>,
    #[serde(default)]
    pub github: Option<GithubConfig>,
    #[serde(default)]
    pub jira: Option<JiraConfig>,
    #[serde(default = "yes")]
    pub local_password: bool,
    #[serde(default = "yes")]
    pub cookie_secure: bool,
    #[serde(default = "landing")]
    pub after_sign_in: String,
}

fn yes() -> bool {
    true
}

fn landing() -> String {
    "/".to_owned()
}

fn a_month() -> u64 {
    30
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpansConfig {
    pub url: String,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "a_month")]
    pub retention_days: u64,
}

impl From<SpansConfig> for SpanStoreSettings {
    fn from(spans: SpansConfig) -> Self {
        Self {
            url: spans.url,
            database: spans.database,
            user: spans.user,
            password: spans.password,
            retention_days: spans.retention_days,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailConfig {
    pub relay: String,
    pub username: String,
    pub password: String,
    pub from: String,
    pub base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

/* A project management tool is wired up by the deployment rather than by an organisation, which
is where the identity provider and the mail relay already live. The alternative was a token
encrypted at rest in the control plane, and that buys one thing — connecting from the screen —
for the price of a key the deployment must never lose. Nothing here holds a secret the database
could leak, and every field is a setting an operator can see in one file.

Each block asks for the least that makes it usable, which is not the same amount for all three:
a Linear key stands alone, a Jira token means nothing without the site it belongs to and the
address that drew it, and a GitHub token wants to know which repository it is for. */
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearConfig {
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubConfig {
    pub token: String,
    /// `owner/name`, the repository an issue is opened in when nobody names another.
    pub repository: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JiraConfig {
    /// The site, such as `https://acme.atlassian.net`.
    pub base_url: String,
    /// The address the api token was drawn for; Jira Cloud authenticates the pair.
    pub email: String,
    pub token: String,
    /// The project key an issue is opened under, such as `OPS`.
    pub project_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantKey {
    pub id: Uuid,
    #[serde(default)]
    pub allowance: TenantAllowance,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "bytes", rename_all = "snake_case")]
pub enum TenantAllowance {
    #[default]
    Unlimited,
    Capped(u64),
    Overage(u64),
}

impl From<TenantAllowance> for Allowance {
    fn from(allowance: TenantAllowance) -> Self {
        match allowance {
            TenantAllowance::Unlimited => Self::Unlimited,
            TenantAllowance::Capped(bytes) => Self::Capped(DecompressedBytes::new(bytes)),
            TenantAllowance::Overage(bytes) => Self::Overage(DecompressedBytes::new(bytes)),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            grpc_address: SocketAddr::from(([0, 0, 0, 0], 4317)),
            http_address: SocketAddr::from(([0, 0, 0, 0], 4318)),
            database_url: None,
            redis_url: None,
            spans: None,
            web_root: None,
            tenants: Vec::new(),
            oidc: None,
            mail: None,
            secret_key: None,
            linear: None,
            github: None,
            jira: None,
            local_password: true,
            cookie_secure: true,
            after_sign_in: landing(),
        }
    }
}

pub fn figment() -> Figment {
    Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file(FILE))
        .merge(Env::prefixed(PREFIX).split("__"))
}

pub fn load() -> anyhow::Result<Config> {
    figment().extract().map_err(Into::into)
}

#[cfg(test)]
#[allow(clippy::result_large_err)]
mod tests {
    use super::{Config, FILE, TenantAllowance, figment};

    #[test]
    fn the_default_ports_are_the_otlp_ones() {
        figment::Jail::expect_with(|_jail| {
            let config: Config = figment().extract()?;

            assert_eq!(config.grpc_address.port(), 4317);
            assert_eq!(config.http_address.port(), 4318);
            assert!(config.tenants.is_empty());
            Ok(())
        });
    }

    #[test]
    fn a_file_declares_tenant_keys() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                FILE,
                r#"
                http_address = "127.0.0.1:9318"

                [[tenants]]
                id = "00000000-0000-0000-0000-000000000001"
                "#,
            )?;

            let config: Config = figment().extract()?;

            assert_eq!(config.http_address.port(), 9318);
            assert_eq!(config.grpc_address.port(), 4317);
            assert_eq!(config.tenants.len(), 1);
            Ok(())
        });
    }

    #[test]
    fn a_tenant_without_an_allowance_is_unlimited() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                FILE,
                r#"
                [[tenants]]
                id = "00000000-0000-0000-0000-000000000001"
                "#,
            )?;

            let config: Config = figment().extract()?;

            assert_eq!(config.tenants[0].allowance, TenantAllowance::Unlimited);
            Ok(())
        });
    }

    #[test]
    fn an_allowance_declares_its_kind_and_its_bytes() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                FILE,
                r#"
                [[tenants]]
                id = "00000000-0000-0000-0000-000000000002"
                allowance = { kind = "capped", bytes = 1048576 }

                [[tenants]]
                id = "00000000-0000-0000-0000-000000000003"
                allowance = { kind = "overage", bytes = 10485760 }
                "#,
            )?;

            let config: Config = figment().extract()?;

            assert_eq!(
                config.tenants[0].allowance,
                TenantAllowance::Capped(1_048_576)
            );
            assert_eq!(
                config.tenants[1].allowance,
                TenantAllowance::Overage(10_485_760)
            );
            Ok(())
        });
    }

    #[test]
    fn a_span_store_needs_only_its_url_and_keeps_a_month() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                FILE,
                r#"
                [spans]
                url = "http://clickhouse:8123"
                "#,
            )?;

            let config: Config = figment().extract()?;
            let spans = config.spans.expect("a span store");

            assert_eq!(spans.url, "http://clickhouse:8123");
            assert_eq!(spans.retention_days, 30);
            assert_eq!(spans.database, None);
            Ok(())
        });
    }

    #[test]
    fn a_retention_in_the_file_wins_over_the_default() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                FILE,
                r#"
                [spans]
                url = "http://clickhouse:8123"
                database = "comettrace"
                retention_days = 7
                "#,
            )?;

            let config: Config = figment().extract()?;
            let spans = config.spans.expect("a span store");

            assert_eq!(spans.retention_days, 7);
            assert_eq!(spans.database.as_deref(), Some("comettrace"));
            Ok(())
        });
    }

    #[test]
    fn a_mail_relay_is_declared_whole_or_not_at_all() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                FILE,
                r#"
                [mail]
                relay = "mail.example.com"
                username = "postmaster"
                password = "secret"
                from = "Comettrace <no-reply@example.com>"
                base_url = "https://app.example.com"
                "#,
            )?;

            let config: Config = figment().extract()?;
            let mail = config.mail.expect("a relay");

            assert_eq!(mail.relay, "mail.example.com");
            assert_eq!(mail.base_url, "https://app.example.com");
            Ok(())
        });
    }

    /* A deployment that sets no key is the ordinary case, not a broken one: everything but
    holding a token for somebody else's service works without it. */
    #[test]
    fn a_deployment_with_no_key_is_the_default() {
        figment::Jail::expect_with(|_jail| {
            let config: Config = figment().extract()?;

            assert_eq!(config.secret_key, None);
            Ok(())
        });
    }

    #[test]
    fn a_key_is_read_from_the_file_and_from_the_environment() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(FILE, "secret_key = \"ab\"\n")?;
            assert_eq!(
                figment().extract::<Config>()?.secret_key.as_deref(),
                Some("ab")
            );

            jail.set_env("COMETTRACE_SECRET_KEY", "cd");
            assert_eq!(
                figment().extract::<Config>()?.secret_key.as_deref(),
                Some("cd"),
                "the environment wins over the file, as it does for everything else"
            );
            Ok(())
        });
    }

    #[test]
    fn a_deployment_wired_to_no_tracker_declares_none_of_the_three() {
        figment::Jail::expect_with(|_jail| {
            let config: Config = figment().extract()?;

            assert_eq!(config.linear, None);
            assert_eq!(config.github, None);
            assert_eq!(config.jira, None);
            Ok(())
        });
    }

    #[test]
    fn a_tracker_asks_for_the_least_that_makes_it_usable() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                FILE,
                r#"
                [linear]
                token = "lin_api_secret"

                [github]
                token = "ghp_secret"
                repository = "acme/storefront"

                [jira]
                base_url = "https://acme.atlassian.net"
                email = "operator@example.com"
                token = "jira_secret"
                project_key = "OPS"
                "#,
            )?;

            let config: Config = figment().extract()?;

            assert_eq!(config.linear.expect("a key").token, "lin_api_secret");
            assert_eq!(
                config.github.expect("a repository").repository,
                "acme/storefront"
            );
            let jira = config.jira.expect("a site");
            assert_eq!(jira.base_url, "https://acme.atlassian.net");
            assert_eq!(jira.project_key, "OPS");
            Ok(())
        });
    }

    /* The spelling the deployment README promises, which is the part somebody types by hand and
    the part a typo is silent in: a block and its field, split on the double underscore. */
    #[test]
    fn a_tracker_is_declared_by_environment_variable_too() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("COMETTRACE_JIRA__BASE_URL", "https://acme.atlassian.net");
            jail.set_env("COMETTRACE_JIRA__EMAIL", "operator@example.com");
            jail.set_env("COMETTRACE_JIRA__TOKEN", "jira_secret");
            jail.set_env("COMETTRACE_JIRA__PROJECT_KEY", "OPS");
            jail.set_env("COMETTRACE_LINEAR__TOKEN", "lin_api_secret");

            let config: Config = figment().extract()?;

            assert_eq!(config.jira.expect("a site").project_key, "OPS");
            assert_eq!(config.linear.expect("a key").token, "lin_api_secret");
            assert_eq!(config.github, None);
            Ok(())
        });
    }
}
