use std::net::SocketAddr;

use comettrace_core::DecompressedBytes;
use comettrace_storage::Allowance;
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
    pub tenants: Vec<TenantKey>,
    #[serde(default)]
    pub oidc: Option<OidcConfig>,
    #[serde(default)]
    pub mail: Option<MailConfig>,
    #[serde(default = "yes")]
    pub local_password: bool,
    #[serde(default = "yes")]
    pub cookie_secure: bool,
    #[serde(default = "console")]
    pub after_sign_in: String,
}

fn yes() -> bool {
    true
}

fn console() -> String {
    "/console".to_owned()
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
            tenants: Vec::new(),
            oidc: None,
            mail: None,
            local_password: true,
            cookie_secure: true,
            after_sign_in: console(),
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
}
