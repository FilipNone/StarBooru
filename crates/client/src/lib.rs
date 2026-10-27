//! Typed client for the StarBooru HTTP API.
//!
//! Works on native targets (rustls) and on `wasm32-unknown-unknown`
//! (browser `fetch`), so the web, desktop and Android apps share it.

use reqwest::{Method, RequestBuilder, StatusCode, header};
use serde::Serialize;
use serde::de::DeserializeOwned;
use starbooru_core::{
    API_PREFIX, ApiError, AuthResponse, Credentials, Post, PostPage, PostUpdate, PurgeResult,
    Rating, ServerInfo, Tag, TagUpdate, User, VoteRequest,
};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid server URL: {0}")]
    InvalidUrl(String),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{}", .error.message)]
    Api { status: u16, error: ApiError },
    /// HTTP 429. `retry_after` is in seconds when the server sent one.
    #[error("rate limited, retry later")]
    RateLimited { retry_after: Option<u64> },
}

impl ClientError {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, ClientError::Api { status: 401, .. })
    }

    /// Stable error code from the server, if this is an API error.
    pub fn code(&self) -> Option<&str> {
        match self {
            ClientError::Api { error, .. } => Some(&error.code),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// One file to upload, already read into memory.
pub struct Upload {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub tags: Vec<String>,
    pub rating: Rating,
    pub source: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    token: Option<String>,
}

impl Client {
    /// `base_url` is the instance root, e.g. `https://booru.example.com`.
    pub fn new(base_url: &str) -> Result<Self> {
        let trimmed = base_url.trim().trim_end_matches('/');
        let base = Url::parse(&format!("{trimmed}/"))
            .map_err(|e| ClientError::InvalidUrl(format!("{base_url}: {e}")))?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(ClientError::InvalidUrl(format!(
                "{base_url}: only http and https are supported"
            )));
        }
        Ok(Self {
            http: reqwest::Client::new(),
            base,
            token: None,
        })
    }

    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn base_url(&self) -> &str {
        self.base.as_str().trim_end_matches('/')
    }

    /// Resolves a server-relative URL (e.g. a post's `file_url`).
    pub fn absolute(&self, path: &str) -> String {
        self.base
            .join(path.trim_start_matches('/'))
            .map(String::from)
            .unwrap_or_else(|_| path.to_owned())
    }

    fn request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        let url = self
            .base
            .join(&format!("{}{path}", API_PREFIX.trim_start_matches('/')))
            .map_err(|e| ClientError::InvalidUrl(e.to_string()))?;
        let mut req = self.http.request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        Ok(req)
    }

    async fn send<T: DeserializeOwned>(req: RequestBuilder) -> Result<T> {
        let resp = check(req.send().await?).await?;
        Ok(resp.json().await?)
    }

    async fn send_empty(req: RequestBuilder) -> Result<()> {
        check(req.send().await?).await?;
        Ok(())
    }

    async fn json<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<T> {
        Self::send(self.request(method, path)?.json(body)).await
    }

    pub async fn info(&self) -> Result<ServerInfo> {
        Self::send(self.request(Method::GET, "/info")?).await
    }

    pub async fn login(&self, creds: &Credentials) -> Result<AuthResponse> {
        self.json(Method::POST, "/auth/login", creds).await
    }

    pub async fn register(&self, creds: &Credentials) -> Result<AuthResponse> {
        self.json(Method::POST, "/auth/register", creds).await
    }

    pub async fn logout(&self) -> Result<()> {
        Self::send_empty(self.request(Method::POST, "/auth/logout")?).await
    }

    pub async fn me(&self) -> Result<User> {
        Self::send(self.request(Method::GET, "/auth/me")?).await
    }

    pub async fn search_posts(&self, query: &str, page: u32, limit: u32) -> Result<PostPage> {
        let req = self.request(Method::GET, "/posts")?.query(&[
            ("q", query.to_owned()),
            ("page", page.to_string()),
            ("limit", limit.to_string()),
        ]);
        Self::send(req).await
    }

    pub async fn post(&self, id: i64) -> Result<Post> {
        Self::send(self.request(Method::GET, &format!("/posts/{id}"))?).await
    }

    pub async fn update_post(&self, id: i64, update: &PostUpdate) -> Result<Post> {
        self.json(Method::PATCH, &format!("/posts/{id}"), update)
            .await
    }

    pub async fn delete_post(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::DELETE, &format!("/posts/{id}"))?).await
    }

    pub async fn restore_post(&self, id: i64) -> Result<Post> {
        Self::send(self.request(Method::POST, &format!("/posts/{id}/restore"))?).await
    }

    /// Permanently deletes a post that is already in the recycle bin.
    pub async fn purge_post(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::POST, &format!("/posts/{id}/purge"))?).await
    }

    /// Admins: purges every post in the recycle bin.
    pub async fn empty_trash(&self) -> Result<PurgeResult> {
        Self::send(self.request(Method::POST, "/trash/empty")?).await
    }

    pub async fn vote(&self, id: i64, value: i16) -> Result<Post> {
        self.json(
            Method::PUT,
            &format!("/posts/{id}/vote"),
            &VoteRequest { value },
        )
        .await
    }

    pub async fn upload(&self, upload: Upload) -> Result<Post> {
        let file = reqwest::multipart::Part::bytes(upload.bytes).file_name(upload.file_name);
        let mut form = reqwest::multipart::Form::new()
            .part("file", file)
            .text("tags", upload.tags.join(" "))
            .text("rating", upload.rating.as_str());
        if let Some(source) = upload.source {
            form = form.text("source", source);
        }
        Self::send(self.request(Method::POST, "/posts")?.multipart(form)).await
    }

    /// Tag autocomplete: tags starting with `prefix`, most used first.
    pub async fn tags(&self, prefix: &str, limit: u32) -> Result<Vec<Tag>> {
        let req = self
            .request(Method::GET, "/tags")?
            .query(&[("q", prefix.to_owned()), ("limit", limit.to_string())]);
        Self::send(req).await
    }

    pub async fn update_tag(&self, name: &str, update: &TagUpdate) -> Result<Tag> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        self.json(Method::PATCH, &format!("/tags/{name}"), update)
            .await
    }
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = resp
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok());
        return Err(ClientError::RateLimited { retry_after });
    }
    let error = resp.json::<ApiError>().await.unwrap_or_else(|_| ApiError {
        code: "http_error".into(),
        message: format!("server answered {status}"),
    });
    Err(ClientError::Api {
        status: status.as_u16(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_urls() {
        assert!(Client::new("not a url").is_err());
        assert!(Client::new("ftp://x").is_err());
        assert!(Client::new("").is_err());
    }

    #[test]
    fn resolves_media_paths() {
        let c = Client::new("https://b.example.com/sub/").unwrap();
        assert_eq!(c.base_url(), "https://b.example.com/sub");
        assert_eq!(
            c.absolute("/media/thumb/ab.jpg?sig=1"),
            "https://b.example.com/sub/media/thumb/ab.jpg?sig=1"
        );
    }
}
