use comettrace_core::{Authorized, ConnectionError, Handshake, Provider, ProviderToken};
use serde::{Deserialize, Serialize};

const AUTHORIZE: &str = "https://linear.app/oauth/authorize";
const TOKEN: &str = "https://api.linear.app/oauth/token";
const GRAPHQL: &str = "https://api.linear.app/graphql";

/* Enough to read an issue and open one, and nothing else. A connection that could also delete is
a connection that will, the day somebody writes the wrong loop. */
const SCOPE: &str = "read,write";

#[derive(Debug, Clone)]
pub struct LinearSettings {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

/* What gets sealed. The access token is the part that matters, and the expiry rides along because
it is the one thing that decides whether a stored credential is still worth sending — Linear hands
back something like ten years, but a credential that cannot say when it dies is a credential
nobody can refresh. */
#[derive(Debug, Serialize, Deserialize)]
struct Credential {
    access_token: String,
    expires_at_unix_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Granted {
    access_token: String,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Answered {
    data: Option<Viewed>,
}

#[derive(Debug, Deserialize)]
struct Viewed {
    organization: Named,
}

#[derive(Debug, Deserialize)]
struct Named {
    name: String,
}

#[derive(Debug, Clone)]
pub struct LinearHandshake {
    settings: LinearSettings,
    http: reqwest::Client,
}

impl LinearHandshake {
    pub fn new(settings: LinearSettings) -> Self {
        Self {
            settings,
            http: reqwest::Client::new(),
        }
    }

    /* Asked of Linear rather than typed by anybody, because a connection somebody labelled
    themselves would drift from the workspace it actually reaches. */
    async fn workspace(&self, access_token: &str) -> Result<String, ConnectionError> {
        let answered = self
            .http
            .post(GRAPHQL)
            .bearer_auth(access_token)
            .json(&serde_json::json!({ "query": "{ organization { name } }" }))
            .send()
            .await
            .map_err(|error| unreachable(&error, "name the workspace"))?;

        if !answered.status().is_success() {
            tracing::error!(status = %answered.status(), "linear refused to name the workspace");
            return Err(ConnectionError::ProviderRefused);
        }

        let read: Answered = answered.json().await.map_err(|error| {
            tracing::error!(%error, "linear answered something that is not the workspace");
            ConnectionError::ProviderRefused
        })?;
        read.data
            .map(|viewed| viewed.organization.name)
            .ok_or(ConnectionError::ProviderRefused)
    }
}

impl Handshake for LinearHandshake {
    fn authorize_url(&self, provider: Provider, state: &str) -> Result<String, ConnectionError> {
        if provider != Provider::Linear {
            return Err(ConnectionError::ProviderUnconfigured);
        }
        /* The redirect uri goes out encoded and comes back compared: Linear refuses a code whose
        uri does not match the one the authorize call carried, character for character. */
        Ok(format!(
            "{AUTHORIZE}?response_type=code&client_id={}&redirect_uri={}&scope={SCOPE}&state={state}",
            encoded(&self.settings.client_id),
            encoded(&self.settings.redirect_uri),
        ))
    }

    async fn finish(&self, provider: Provider, code: &str) -> Result<Authorized, ConnectionError> {
        if provider != Provider::Linear {
            return Err(ConnectionError::ProviderUnconfigured);
        }

        let answered = self
            .http
            .post(TOKEN)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.settings.client_id),
                ("client_secret", &self.settings.client_secret),
                ("redirect_uri", &self.settings.redirect_uri),
            ])
            .send()
            .await
            .map_err(|error| unreachable(&error, "exchange the code"))?;

        /* A refusal here is the ordinary case rather than a fault: a code is spent once and dies
        in minutes, so somebody who reloads the callback lands exactly here. The status is logged
        and the body is not — it is Linear's, and it has carried the client secret back before. */
        if !answered.status().is_success() {
            tracing::warn!(status = %answered.status(), "linear refused the code");
            return Err(ConnectionError::ProviderRefused);
        }

        let granted: Granted = answered.json().await.map_err(|error| {
            tracing::error!(%error, "linear answered something that is not a token");
            ConnectionError::ProviderRefused
        })?;

        let account_label = self.workspace(&granted.access_token).await?;
        let credential = Credential {
            expires_at_unix_seconds: granted.expires_in.map(|seconds| now_seconds() + seconds),
            access_token: granted.access_token,
        };
        let sealed = serde_json::to_string(&credential).map_err(|error| {
            tracing::error!(%error, "a credential could not be written down");
            ConnectionError::ProviderRefused
        })?;

        Ok(Authorized {
            credential: ProviderToken::new(&sealed),
            account_label: account_label.into(),
        })
    }
}

/* Only what a query string needs, and spelt out rather than pulled in: the two values that reach
it here are a client id and a url this deployment wrote itself. */
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

fn unreachable(error: &reqwest::Error, what: &str) -> ConnectionError {
    tracing::error!(%error, "linear could not be reached to {what}");
    ConnectionError::ProviderUnreachable
}

#[cfg(test)]
mod tests {
    use comettrace_core::{ConnectionError, Handshake, Provider};

    use super::{LinearHandshake, LinearSettings, encoded};

    fn a_handshake() -> LinearHandshake {
        LinearHandshake::new(LinearSettings {
            client_id: "an-application".to_owned(),
            client_secret: "a-secret".to_owned(),
            redirect_uri: "http://localhost:4318/api/integrations/linear/callback".to_owned(),
        })
    }

    #[test]
    fn the_authorize_url_carries_the_state_and_the_encoded_redirect() {
        let url = a_handshake()
            .authorize_url(Provider::Linear, "a-state")
            .expect("linear is the one it is for");

        assert!(
            url.starts_with("https://linear.app/oauth/authorize?"),
            "{url}"
        );
        assert!(url.contains("state=a-state"), "{url}");
        assert!(url.contains("response_type=code"), "{url}");
        assert!(url.contains("client_id=an-application"), "{url}");
        assert!(
            url.contains(
                "redirect_uri=http%3A%2F%2Flocalhost%3A4318%2Fapi%2Fintegrations%2Flinear%2Fcallback"
            ),
            "the redirect has to survive the query string verbatim: {url}"
        );
    }

    /* It asks for what it needs to open an issue and read one back, and nothing that could
    delete. */
    #[test]
    fn the_scope_asked_for_is_read_and_write() {
        let url = a_handshake()
            .authorize_url(Provider::Linear, "a-state")
            .expect("the url");

        assert!(url.contains("scope=read,write"), "{url}");
    }

    /* One adapter answers for one provider. A dispatcher that handed a Jira flow to this would be
    a bug, so it is refused rather than quietly building a Linear url. */
    #[test]
    fn another_provider_is_not_this_adapters_to_answer_for() {
        for provider in [Provider::Github, Provider::Jira] {
            assert_eq!(
                a_handshake().authorize_url(provider, "a-state").err(),
                Some(ConnectionError::ProviderUnconfigured)
            );
        }
    }

    #[test]
    fn a_secret_never_reaches_the_authorize_url() {
        let url = a_handshake()
            .authorize_url(Provider::Linear, "a-state")
            .expect("the url");

        assert!(!url.contains("a-secret"), "{url}");
    }

    #[test]
    fn encoding_leaves_the_unreserved_characters_alone_and_escapes_the_rest() {
        assert_eq!(encoded("abcXYZ019-_.~"), "abcXYZ019-_.~");
        assert_eq!(encoded("a b"), "a%20b");
        assert_eq!(
            encoded("http://x/y?z=1&w"),
            "http%3A%2F%2Fx%2Fy%3Fz%3D1%26w"
        );
    }
}
