use core::fmt;

use comettrace_core::{Clock, Photo, PhotoError, Photos, Pictured, Portrait};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, Method, StatusCode, Url};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/* Signature Version 4, by hand. The project already carries reqwest, sha2, hex, time and rand,
and hmac is already in the tree, so signing three request shapes costs nothing new -- where an
aws sdk would bring its own runtime, its own http stack and some fifty crates to put a megabyte
somewhere. R2 is what this was built against; the protocol is S3, so an S3 bucket or a MinIO also
answer it. */
const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const SERVICE: &str = "s3";
/* The hash of no bytes at all, which a GET and a DELETE both carry. */
const NOTHING: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
/* R2 has one region and calls it this. A real S3 bucket names its own. */
const ANY_REGION: &str = "auto";
const TOKEN_BYTES: usize = 8;
/* Two prefixes in one bucket rather than a bucket per kind: a bucket is an operator's to create,
and a deployment should need exactly one. The prefix is what keeps an account's face and an
organisation's mark from ever naming the same object when the two ids happen to collide. */
const AVATARS: &str = "avatars";
const LOGOS: &str = "logos";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoStoreSettings {
    pub endpoint: String,
    pub bucket: String,
    pub region: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PhotoStoreError {
    #[error("the photo store endpoint is not a url: {0}")]
    Endpoint(String),
    #[error("the photo store endpoint carries no host: {0}")]
    Host(String),
}

pub struct S3Photos<C> {
    http: Client,
    endpoint: Url,
    host: String,
    bucket: String,
    region: String,
    key_id: String,
    secret: String,
    clock: C,
}

/* The endpoint and the bucket, because those are what an operator mistyped. Never the key. */
impl<C> fmt::Debug for S3Photos<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("S3Photos")
            .field("endpoint", &self.endpoint.as_str())
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl<C: Clock> S3Photos<C> {
    pub fn new(settings: &PhotoStoreSettings, clock: C) -> Result<Self, PhotoStoreError> {
        let endpoint = Url::parse(&settings.endpoint)
            .map_err(|_| PhotoStoreError::Endpoint(settings.endpoint.clone()))?;
        let host =
            authority(&endpoint).ok_or_else(|| PhotoStoreError::Host(settings.endpoint.clone()))?;
        Ok(Self {
            http: Client::new(),
            endpoint,
            host,
            bucket: settings.bucket.clone(),
            region: settings
                .region
                .clone()
                .unwrap_or_else(|| ANY_REGION.to_owned()),
            key_id: settings.access_key_id.clone(),
            secret: settings.secret_access_key.clone(),
            clock,
        })
    }

    /* Path style, with the bucket ahead of the object: R2 answers it, a MinIO answers it, and an
    S3 bucket whose name is not a legal hostname answers only this. */
    fn canonical_uri(&self, object: &str) -> String {
        format!("/{}/{}", self.bucket, object)
    }

    fn signature_instant(&self) -> (String, String) {
        let seconds = i64::try_from(self.clock.now().get()).unwrap_or_default();
        let now =
            OffsetDateTime::from_unix_timestamp(seconds).unwrap_or(OffsetDateTime::UNIX_EPOCH);
        let stamp = format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );
        let day = stamp[..8].to_owned();
        (stamp, day)
    }

    async fn send(
        &self,
        method: Method,
        object: &str,
        body: Option<&Photo>,
    ) -> Result<(StatusCode, Vec<u8>), PhotoError> {
        let (stamp, day) = self.signature_instant();
        let uri = self.canonical_uri(object);
        let payload = body.map_or_else(|| NOTHING.to_owned(), |photo| hashed(photo.bytes()));

        let mut headers = vec![
            ("host", self.host.clone()),
            ("x-amz-content-sha256", payload.clone()),
            ("x-amz-date", stamp.clone()),
        ];
        if let Some(photo) = body {
            headers.push(("content-type", photo.kind().as_str().to_owned()));
        }
        let scope = format!("{day}/{}/{SERVICE}/aws4_request", self.region);
        let canonical = canonical_request(method.as_str(), &uri, &headers, &payload);
        let signature = hex::encode(self.sign(&string_to_sign(&stamp, &scope, &canonical), &day));
        let authorization = format!(
            "{ALGORITHM} Credential={}/{scope}, SignedHeaders={}, Signature={signature}",
            self.key_id,
            signed_headers(&headers)
        );

        let mut url = self.endpoint.clone();
        url.set_path(&uri);
        let mut request = self
            .http
            .request(method, url)
            .header("x-amz-content-sha256", &payload)
            .header("x-amz-date", &stamp)
            .header("authorization", authorization);
        if let Some(photo) = body {
            request = request
                .header("content-type", photo.kind().as_str())
                .body(photo.bytes().to_vec());
        }

        let response = request.send().await.map_err(|error| {
            tracing::error!(%error, "the photo store could not be reached");
            PhotoError::Unreachable
        })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|error| {
            tracing::error!(%error, "the photo store answered a body we could not read");
            PhotoError::Unreachable
        })?;
        Ok((status, bytes.to_vec()))
    }

    fn sign(&self, to_sign: &str, day: &str) -> [u8; 32] {
        let start = format!("AWS4{}", self.secret);
        let date = hmac(start.as_bytes(), day.as_bytes());
        let region = hmac(&date, self.region.as_bytes());
        let service = hmac(&region, SERVICE.as_bytes());
        let signing = hmac(&service, b"aws4_request");
        hmac(&signing, to_sign.as_bytes())
    }
}

impl<C: Clock + Send + Sync> Photos for S3Photos<C> {
    async fn put(&self, of: Pictured, photo: &Photo) -> Result<Portrait, PhotoError> {
        let portrait = Portrait {
            token: mint().into(),
            kind: photo.kind(),
        };
        let object = object_of(of, &portrait);
        let (status, body) = self.send(Method::PUT, &object, Some(photo)).await?;
        if !status.is_success() {
            tracing::error!(
                %status,
                object = %object,
                answer = %String::from_utf8_lossy(&body),
                "the photo store refused to keep a photo"
            );
            return Err(PhotoError::Unreachable);
        }
        Ok(portrait)
    }

    async fn fetch(
        &self,
        of: Pictured,
        portrait: &Portrait,
    ) -> Result<Option<Vec<u8>>, PhotoError> {
        let object = object_of(of, portrait);
        let (status, body) = self.send(Method::GET, &object, None).await?;
        if status == StatusCode::NOT_FOUND {
            /* The column pointed at an object the store no longer holds. That is a photo that is
            gone rather than a store that is broken, so the screen falls back to initials. */
            tracing::warn!(object = %object, "the photo store holds no such object any more");
            return Ok(None);
        }
        if !status.is_success() {
            tracing::error!(
                %status,
                object = %object,
                answer = %String::from_utf8_lossy(&body),
                "the photo store refused to hand back a photo"
            );
            return Err(PhotoError::Unreachable);
        }
        Ok(Some(body))
    }

    /* A delete that finds nothing has done its job: what was asked for is an object that is not
    there. */
    async fn remove(&self, of: Pictured, portrait: &Portrait) -> Result<(), PhotoError> {
        let object = object_of(of, portrait);
        let (status, body) = self.send(Method::DELETE, &object, None).await?;
        if !status.is_success() && status != StatusCode::NOT_FOUND {
            tracing::error!(
                %status,
                object = %object,
                answer = %String::from_utf8_lossy(&body),
                "the photo store refused to forget a photo"
            );
            return Err(PhotoError::Unreachable);
        }
        Ok(())
    }
}

/* One object per picture rather than one per owner, because the token is what lets a url be
cached for ever: a replacement is a different object, so no reader is ever served the face
somebody deleted. The owner is in the path, under the prefix its kind answers to, so a bucket can
be read by eye and the two kinds never collide. */
fn object_of(of: Pictured, portrait: &Portrait) -> String {
    let (prefix, owner) = match of {
        Pictured::Account(account) => (AVATARS, account.as_uuid()),
        Pictured::Org(org) => (LOGOS, org.as_uuid()),
    };
    format!(
        "{prefix}/{owner}/{}.{}",
        portrait.token,
        portrait.kind.extension()
    )
}

fn mint() -> String {
    let bytes: [u8; TOKEN_BYTES] = rand::random();
    hex::encode(bytes)
}

fn authority(endpoint: &Url) -> Option<String> {
    let host = endpoint.host_str()?;
    /* The port only belongs in the header when the url carries one that is not the scheme's
    default, which is exactly when `Url::port` answers. A signature over the wrong host is a 403
    nobody can read. */
    Some(match endpoint.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

fn hashed(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac accepts a key of any length");
    mac.update(data);
    let out = mac.finalize().into_bytes();
    let mut signature = [0u8; 32];
    signature.copy_from_slice(&out);
    signature
}

fn signed_headers(headers: &[(&str, String)]) -> String {
    let mut names: Vec<&str> = headers.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    names.join(";")
}

/* The canonical request, as the signature algorithm spells it: the method, the path, the query
string, every signed header in order, the names of those headers, and the hash of the body. The
query string is empty here because none of the three requests carries one. */
fn canonical_request(method: &str, uri: &str, headers: &[(&str, String)], payload: &str) -> String {
    let mut sorted: Vec<&(&str, String)> = headers.iter().collect();
    sorted.sort_by(|left, right| left.0.cmp(right.0));
    let named: String = sorted
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect();
    format!(
        "{method}\n{uri}\n\n{named}\n{}\n{payload}",
        signed_headers(headers)
    )
}

fn string_to_sign(stamp: &str, scope: &str, canonical: &str) -> String {
    format!(
        "{ALGORITHM}\n{stamp}\n{scope}\n{}",
        hashed(canonical.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use comettrace_core::{AccountId, Photo, PhotoKind, Pictured, Portrait, TenantId};
    use uuid::Uuid;

    use super::{
        NOTHING, PhotoStoreSettings, S3Photos, canonical_request, mint, object_of, string_to_sign,
    };
    use crate::clock::FixedClock;

    const NOON: u64 = 1_700_000_000;

    fn settings() -> PhotoStoreSettings {
        PhotoStoreSettings {
            endpoint: "https://abc123.r2.cloudflarestorage.com".to_owned(),
            bucket: "comettrace".to_owned(),
            region: None,
            access_key_id: "AKIAEXAMPLE".to_owned(),
            secret_access_key: "a secret nobody prints".to_owned(),
        }
    }

    fn store() -> S3Photos<FixedClock> {
        S3Photos::new(&settings(), FixedClock::at(NOON)).expect("a store")
    }

    fn an_account() -> Pictured {
        Pictured::Account(AccountId::new(Uuid::from_u128(7)))
    }

    fn an_org() -> Pictured {
        Pictured::Org(TenantId::new(Uuid::from_u128(7)))
    }

    fn a_portrait() -> Portrait {
        Portrait {
            token: "deadbeefdeadbeef".into(),
            kind: PhotoKind::Png,
        }
    }

    #[test]
    fn an_endpoint_that_is_not_a_url_is_refused_at_startup() {
        let mut settings = settings();
        settings.endpoint = "not a url".to_owned();

        assert!(S3Photos::new(&settings, FixedClock::at(NOON)).is_err());
    }

    #[test]
    fn the_region_defaults_to_the_one_r2_answers_to() {
        assert!(format!("{:?}", store()).contains("auto"));
    }

    /* The one thing that must never be renderable. */
    #[test]
    fn a_store_never_renders_its_secret() {
        let rendered = format!("{:?}", store());

        assert!(!rendered.contains("a secret nobody prints"), "{rendered}");
        assert!(rendered.contains("comettrace"));
    }

    #[test]
    fn an_object_is_named_after_the_owner_the_token_and_the_kind() {
        assert_eq!(
            object_of(an_account(), &a_portrait()),
            "avatars/00000000-0000-0000-0000-000000000007/deadbeefdeadbeef.png"
        );
    }

    /* The prefix is the whole of what separates the two kinds, so the one case that would be a
    silent mix-up is pinned: the same uuid worn by an account and by an organisation. */
    #[test]
    fn an_account_and_an_organisation_of_the_same_id_never_name_the_same_object() {
        let face = object_of(an_account(), &a_portrait());
        let mark = object_of(an_org(), &a_portrait());

        assert_eq!(
            mark,
            "logos/00000000-0000-0000-0000-000000000007/deadbeefdeadbeef.png"
        );
        assert_ne!(face, mark);
    }

    #[test]
    fn a_minted_token_is_hex_and_two_of_them_differ() {
        let token = mint();

        assert_eq!(token.len(), 16);
        assert!(token.chars().all(|letter| letter.is_ascii_hexdigit()));
        assert_ne!(mint(), mint());
    }

    #[test]
    fn the_bucket_comes_ahead_of_the_object_in_the_path() {
        assert_eq!(
            store().canonical_uri("avatars/x/y.png"),
            "/comettrace/avatars/x/y.png"
        );
    }

    #[test]
    fn the_stamp_is_the_basic_iso_form_the_algorithm_asks_for() {
        let (stamp, day) = store().signature_instant();

        assert_eq!(stamp, "20231114T221320Z");
        assert_eq!(day, "20231114");
    }

    /* The shape the algorithm hashes. Everything below the signature is arithmetic; this is where
    a mistake turns into a 403 nobody can read, so it is pinned by eye. */
    #[test]
    fn the_canonical_request_is_the_shape_the_algorithm_hashes() {
        let headers = vec![
            ("x-amz-date", "20231114T221320Z".to_owned()),
            ("host", "abc123.r2.cloudflarestorage.com".to_owned()),
            ("x-amz-content-sha256", NOTHING.to_owned()),
        ];

        let canonical = canonical_request("GET", "/comettrace/avatars/x/y.png", &headers, NOTHING);

        assert_eq!(
            canonical,
            format!(
                "GET\n\
                 /comettrace/avatars/x/y.png\n\
                 \n\
                 host:abc123.r2.cloudflarestorage.com\n\
                 x-amz-content-sha256:{NOTHING}\n\
                 x-amz-date:20231114T221320Z\n\
                 \n\
                 host;x-amz-content-sha256;x-amz-date\n\
                 {NOTHING}"
            ),
            "the headers are signed in order, whatever order they were given in"
        );
    }

    #[test]
    fn the_string_to_sign_names_the_algorithm_the_instant_and_the_scope() {
        let lines: Vec<String> = string_to_sign(
            "20231114T221320Z",
            "20231114/auto/s3/aws4_request",
            "whatever the canonical request was",
        )
        .lines()
        .map(str::to_owned)
        .collect();

        assert_eq!(lines[0], "AWS4-HMAC-SHA256");
        assert_eq!(lines[1], "20231114T221320Z");
        assert_eq!(lines[2], "20231114/auto/s3/aws4_request");
        assert_eq!(lines[3].len(), 64, "the canonical request arrives hashed");
    }

    /* The payload hash is the body's, and a request with no body carries the hash of no bytes --
    not the words, the hash. */
    #[test]
    fn a_body_is_hashed_and_an_empty_one_has_a_known_hash() {
        let photo = Photo::examined(vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, b'f', b'a', b'c', b'e',
        ])
        .expect("a png");

        assert_eq!(super::hashed(b""), NOTHING);
        assert_ne!(super::hashed(photo.bytes()), NOTHING);
        assert_eq!(super::hashed(photo.bytes()).len(), 64);
    }
}
