//! Sign-in through an OpenID Connect provider: the authorization code flow
//! with PKCE, for a confidential client. The ID token comes straight from
//! the provider's token endpoint over TLS, authenticated with the client
//! secret, so its claims are checked but its signature isn't (OpenID
//! Connect Core 3.1.3.7, item 6).
//!
//! The client side of the flow is described in `starbooru_core::oidc`.

use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use reqwest::redirect;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use url::Url;

use crate::config::OidcConfig;
use crate::error::{AppError, AppResult};

const USER_AGENT: &str = concat!("StarBooru/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(15);
/// Provider metadata is fetched again after this long.
const METADATA_TTL: Duration = Duration::from_secs(3600);
/// Largest provider answer read.
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
/// Clock difference allowed when checking the token's expiry.
const SKEW_SECS: i64 = 60;

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    issuer: String,
    authorization_endpoint: Url,
    token_endpoint: Url,
}

pub struct Oidc {
    config: OidcConfig,
    http: reqwest::Client,
    metadata: Mutex<Option<(Instant, Metadata)>>,
}

/// Who signed in, from the ID token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claims {
    pub issuer: String,
    pub subject: String,
    /// A username or email to show, if the provider gave one.
    pub label: Option<String>,
}

impl Oidc {
    pub fn new(config: OidcConfig) -> Result<Self, reqwest::Error> {
        // sqlx already uses ring; reqwest takes the process-wide provider.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .redirect(redirect::Policy::none())
            .build()?;
        Ok(Self {
            config,
            http,
            metadata: Mutex::new(None),
        })
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn public_url(&self) -> &Url {
        &self.config.public_url
    }

    /// The provider's endpoints, from its discovery document.
    pub async fn metadata(&self) -> AppResult<Metadata> {
        let mut cached = self.metadata.lock().await;
        if let Some((at, meta)) = cached.as_ref()
            && at.elapsed() < METADATA_TTL
        {
            return Ok(meta.clone());
        }
        let meta = self.discover().await.map_err(|reason| {
            tracing::warn!("OpenID Connect provider metadata: {reason}");
            AppError::Upstream("the sign-in provider can't be reached; try again later".into())
        })?;
        *cached = Some((Instant::now(), meta.clone()));
        Ok(meta)
    }

    async fn discover(&self) -> Result<Metadata, String> {
        let url = self
            .config
            .issuer
            .join(".well-known/openid-configuration")
            .map_err(|e| e.to_string())?;
        let response = self.http.get(url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("answered {}", response.status()));
        }
        let body = read_capped(response).await?;
        let meta: Metadata = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        if meta.issuer.trim_end_matches('/') != self.config.issuer.as_str().trim_end_matches('/') {
            return Err(format!(
                "its issuer is {}, not the configured one",
                meta.issuer
            ));
        }
        // No plain http unless the issuer itself is (on this machine).
        let scheme = self.config.issuer.scheme();
        if meta.token_endpoint.scheme() != scheme || meta.authorization_endpoint.scheme() != scheme
        {
            return Err("its endpoints don't use the issuer's scheme".into());
        }
        Ok(meta)
    }

    /// The provider's sign-in page for one attempt.
    pub fn authorize_url(&self, meta: &Metadata, state: &str, nonce: &str, verifier: &str) -> Url {
        let mut url = meta.authorization_endpoint.clone();
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", self.config.redirect_uri.as_str())
            .append_pair("scope", "openid profile email")
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", &challenge(verifier))
            .append_pair("code_challenge_method", "S256");
        url
    }

    /// Trades the provider's code for an ID token and checks it. The error
    /// is for the log; it never holds a token.
    pub async fn redeem(&self, code: &str, verifier: &str, nonce: &str) -> Result<Claims, String> {
        let meta = self.metadata().await.map_err(|e| e.to_string())?;
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "authorization_code")
            .append_pair("code", code)
            .append_pair("redirect_uri", self.config.redirect_uri.as_str())
            .append_pair("code_verifier", verifier)
            .finish();
        // client_secret_basic: both parts form-encoded (RFC 6749, 2.3.1).
        let encode =
            |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
        let response = self
            .http
            .post(meta.token_endpoint.clone())
            .basic_auth(
                encode(&self.config.client_id),
                Some(encode(&self.config.client_secret)),
            )
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(reqwest::header::ACCEPT, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("token endpoint: {e}"))?;
        let status = response.status();
        let body = read_capped(response).await?;
        if !status.is_success() {
            // The error code only: the body may echo what was sent.
            let code = serde_json::from_slice::<TokenError>(&body)
                .map(|e| e.error)
                .unwrap_or_default();
            return Err(format!("token endpoint answered {status} {code}"));
        }
        let tokens: TokenResponse =
            serde_json::from_slice(&body).map_err(|_| "token endpoint: no ID token".to_owned())?;
        check_id_token(
            &tokens.id_token,
            &meta.issuer,
            &self.config.client_id,
            nonce,
            Utc::now(),
        )
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
}

#[derive(Deserialize)]
struct IdClaims {
    iss: String,
    sub: String,
    aud: Audience,
    exp: i64,
    nonce: Option<String>,
    azp: Option<String>,
    preferred_username: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn has(&self, id: &str) -> bool {
        match self {
            Audience::One(a) => a == id,
            Audience::Many(all) => all.iter().any(|a| a == id),
        }
    }

    fn len(&self) -> usize {
        match self {
            Audience::One(_) => 1,
            Audience::Many(all) => all.len(),
        }
    }
}

/// Checks the ID token's claims (OpenID Connect Core 3.1.3.7).
fn check_id_token(
    token: &str,
    issuer: &str,
    client_id: &str,
    nonce: &str,
    now: DateTime<Utc>,
) -> Result<Claims, String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| "the ID token is malformed".to_owned())?;
    let json = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .map_err(|_| "the ID token is malformed".to_owned())?;
    let claims: IdClaims =
        serde_json::from_slice(&json).map_err(|e| format!("the ID token's claims: {e}"))?;
    if claims.iss != issuer {
        return Err("the ID token is from another issuer".into());
    }
    if !claims.aud.has(client_id) {
        return Err("the ID token is for another client".into());
    }
    if claims.aud.len() > 1 && claims.azp.as_deref() != Some(client_id) {
        return Err("the ID token was issued to another party".into());
    }
    if claims.exp.saturating_add(SKEW_SECS) < now.timestamp() {
        return Err("the ID token has expired".into());
    }
    if claims.nonce.as_deref() != Some(nonce) {
        return Err("the ID token's nonce doesn't match".into());
    }
    if claims.sub.is_empty() || claims.sub.len() > 255 {
        return Err("the ID token has no usable subject".into());
    }
    let label = claims
        .preferred_username
        .or(claims.email)
        .map(|l| l.trim().chars().take(128).collect::<String>())
        .filter(|l| !l.is_empty() && !l.chars().any(char::is_control));
    Ok(Claims {
        issuer: claims.iss,
        subject: claims.sub,
        label,
    })
}

async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err("the answer is too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// The PKCE S256 challenge for `verifier`.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// `raw` if the code may be sent there: a page of the server's own web app,
/// or a loopback address a native app listens on.
pub fn check_return_to(raw: &str, public_url: &Url) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return None;
    }
    let loopback = url.scheme() == "http"
        && matches!(
            url.host(),
            Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
                | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
                | Some(url::Host::Domain("localhost"))
        );
    let own = url.origin() == public_url.origin() && url.path().starts_with(public_url.path());
    (loopback || own).then_some(url)
}

/// `return_to` with `param=value` added to its query.
pub fn with_param(return_to: &str, param: &str, value: &str) -> String {
    match Url::parse(return_to) {
        Ok(mut url) => {
            url.query_pairs_mut().append_pair(param, value);
            url.into()
        }
        Err(_) => return_to.to_owned(),
    }
}

#[cfg(test)]
pub mod tests {
    use serde_json::json;

    use super::*;

    /// An unsigned token with these claims, as the test provider hands out.
    pub fn id_token(claims: &serde_json::Value) -> String {
        let part = |v: &serde_json::Value| URL_SAFE_NO_PAD.encode(v.to_string());
        format!("{}.{}.c2ln", part(&json!({ "alg": "RS256" })), part(claims))
    }

    #[test]
    fn checks_id_tokens() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let good = json!({
            "iss": "https://auth.example.com/o/booru/",
            "sub": "1234",
            "aud": "booru",
            "exp": 1_800_000_300,
            "nonce": "n0nce",
            "preferred_username": "alice",
            "email": "alice@example.com",
        });
        let check = |claims: &serde_json::Value| {
            check_id_token(
                &id_token(claims),
                "https://auth.example.com/o/booru/",
                "booru",
                "n0nce",
                now,
            )
        };
        assert_eq!(
            check(&good),
            Ok(Claims {
                issuer: "https://auth.example.com/o/booru/".into(),
                subject: "1234".into(),
                label: Some("alice".into()),
            })
        );
        let with = |key: &str, value: serde_json::Value| {
            let mut c = good.clone();
            c[key] = value;
            check(&c)
        };
        assert_eq!(
            with("preferred_username", json!(null))
                .unwrap()
                .label
                .as_deref(),
            Some("alice@example.com")
        );
        assert!(with("iss", json!("https://evil.example/")).is_err());
        assert!(with("aud", json!("other")).is_err());
        assert!(with("aud", json!(["other", "booru"])).is_err());
        assert!(
            check(&json!({
                "iss": "https://auth.example.com/o/booru/", "sub": "1", "exp": 1_800_000_300,
                "nonce": "n0nce", "aud": ["other", "booru"], "azp": "booru"
            }))
            .is_ok()
        );
        assert!(with("exp", json!(1_800_000_000 - SKEW_SECS - 1)).is_err());
        assert!(with("exp", json!(1_800_000_000 - 10)).is_ok());
        assert!(with("nonce", json!("other")).is_err());
        assert!(with("nonce", json!(null)).is_err());
        assert!(with("sub", json!("")).is_err());
        assert!(with("sub", json!(5)).is_err());
        for bad in ["", "abc", "a.!!!.c", "a.e30.c"] {
            assert!(
                check_id_token(bad, "x", "booru", "n0nce", now).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn pkce_challenge() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn return_addresses() {
        let public = Url::parse("https://booru.example.com/").unwrap();
        let ok = |raw: &str| check_return_to(raw, &public).is_some();
        assert!(ok("https://booru.example.com/oidc"));
        assert!(ok("http://127.0.0.1:53111/callback"));
        assert!(ok("http://[::1]:53111/"));
        assert!(ok("http://localhost:9/x"));
        assert!(!ok("https://evil.example/oidc"));
        assert!(!ok("http://booru.example.com/oidc"));
        assert!(!ok("https://booru.example.com.evil.example/"));
        assert!(!ok("https://user@booru.example.com/"));
        assert!(!ok("https://booru.example.com/#frag"));
        assert!(!ok("https://127.0.0.1/"));
        assert!(!ok("myapp://callback"));
        assert!(!ok("/oidc"));
        let under = Url::parse("https://example.com/booru/").unwrap();
        assert!(check_return_to("https://example.com/booru/oidc", &under).is_some());
        assert!(check_return_to("https://example.com/other", &under).is_none());

        assert_eq!(
            with_param("https://booru.example.com/oidc?x=1", "oidc_code", "a b"),
            "https://booru.example.com/oidc?x=1&oidc_code=a+b"
        );
    }
}
