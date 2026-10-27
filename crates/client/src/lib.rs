//! Typed client for the StarBooru HTTP API.
//!
//! Works on native targets (rustls) and on `wasm32-unknown-unknown`
//! (browser `fetch`), so the web, desktop and Android apps share it.

#[cfg(not(target_arch = "wasm32"))]
mod roots;

use reqwest::{Method, RequestBuilder, StatusCode, header};
use serde::Serialize;
use serde::de::DeserializeOwned;
use starbooru_core::{
    API_PREFIX, AdminUser, AliasRequest, ApiError, ApiKey, AuthResponse, Backup, BackupSchedule,
    Backups, BulkEdit, BulkResult, CreatedApiKey, Credentials, DismissDuplicates, Duplicates,
    Explore, Identity, ImplicationRequest, ImportRequest, MediaEdit, Neighbors, NewApiKey,
    NewShare, OidcBegin, OidcFinish, OidcStart, PasswordChange, PasswordReset, Post, PostPage,
    PostUpdate, PurgeResult, Rating, RelatedPosts, RemotePage, RemoteSearch, ResolveDuplicates,
    RevokeResult, SaveSearchRequest, SavedSearch, ServerInfo, ServerSettings, ServerStats,
    SessionInfo, Share, SharedView, SignInMethods, Tag, TagDetail, TagOrder, TagUpdate, Usage,
    User, UserUpdate, VoteRequest,
};
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// How many times a rate-limited request is sent again before giving up.
const RATE_LIMIT_RETRIES: u32 = 2;
/// A longer `Retry-After` than this is returned to the caller instead of
/// waited out, so the UI never hangs on one request.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid server URL: {0}")]
    InvalidUrl(String),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{}", .error.message)]
    Api { status: u16, error: ApiError },
    /// HTTP 429 that is still there after the automatic retries.
    /// `retry_after` is in seconds when the server sent one.
    #[error("{}", rate_limited_message(*.retry_after))]
    RateLimited { retry_after: Option<u64> },
    /// A download was bigger than the caller allowed.
    #[error("the file is larger than {limit} bytes")]
    TooLarge { limit: u64 },
    /// Writing a download to disk failed.
    #[error("cannot save the file: {0}")]
    Io(#[from] std::io::Error),
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

fn rate_limited_message(retry_after: Option<u64>) -> String {
    match retry_after.map(|s| s.div_ceil(60)) {
        None => "too many requests, try again later".to_owned(),
        Some(0 | 1) => "too many attempts, try again in a minute".to_owned(),
        Some(m) => format!("too many attempts, try again in {m} minutes"),
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

/// Native builds name themselves generically (app, version and OS) so the
/// server can label the session; browsers send their own User-Agent.
#[cfg(not(target_arch = "wasm32"))]
fn http_client() -> Result<reqwest::Client> {
    let agent = format!(
        "{}/{} ({})",
        starbooru_core::agent::APP_AGENT,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    let builder = reqwest::Client::builder().user_agent(agent);
    // Android's platform verifier needs JNI setup the app doesn't do;
    // trust the system's CA folder instead (see `roots`).
    #[cfg(target_os = "android")]
    let builder = builder.tls_certs_only(roots::first_store(&roots::ANDROID_CA_DIRS));
    Ok(builder.build()?)
}

#[cfg(target_arch = "wasm32")]
fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::new())
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
            http: http_client()?,
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

    /// Downloads a signed media path as the API returned it
    /// (`/media/…?exp=…&sig=…`) from this server, up to `max_bytes`. The
    /// signature is the permission, so the token is not sent.
    pub async fn media_bytes(&self, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
        if !path.starts_with("/media/") {
            return Err(ClientError::InvalidUrl(format!("{path}: not a media path")));
        }
        let too_large = ClientError::TooLarge { limit: max_bytes };
        #[allow(unused_mut, reason = "read in chunks natively only")]
        let mut resp = self.signed(path).await?;
        if resp.content_length().is_some_and(|n| n > max_bytes) {
            return Err(too_large);
        }
        // Natively, stop reading once past the limit, whatever the length
        // said. The browser has no chunked reads.
        #[cfg(not(target_arch = "wasm32"))]
        let body = {
            let mut body = Vec::new();
            while let Some(chunk) = resp.chunk().await? {
                if body.len().saturating_add(chunk.len()) as u64 > max_bytes {
                    return Err(too_large);
                }
                body.extend_from_slice(&chunk);
            }
            body
        };
        #[cfg(target_arch = "wasm32")]
        let body = resp.bytes().await?.to_vec();
        if body.len() as u64 > max_bytes {
            return Err(too_large);
        }
        Ok(body)
    }

    /// Saves a signed media or backup path (`/media/…`, `/backups/…`) as
    /// `dest`, streaming it through `<dest>.part`. Returns the size.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn download_to(&self, path: &str, dest: &std::path::Path) -> Result<u64> {
        use tokio::io::AsyncWriteExt;
        if !path.starts_with("/media/") && !path.starts_with("/backups/") {
            return Err(ClientError::InvalidUrl(format!("{path}: not a download")));
        }
        let mut resp = self.signed(path).await?;
        let mut part = dest.as_os_str().to_owned();
        part.push(".part");
        let part = std::path::PathBuf::from(part);
        let written = async {
            let mut file = tokio::fs::File::create(&part).await?;
            let mut size = 0u64;
            while let Some(chunk) = resp.chunk().await? {
                file.write_all(&chunk).await?;
                size = size.saturating_add(chunk.len() as u64);
            }
            file.flush().await?;
            file.sync_all().await?;
            tokio::fs::rename(&part, dest).await?;
            Ok(size)
        }
        .await;
        if written.is_err() {
            let _ = tokio::fs::remove_file(&part).await;
        }
        written
    }

    /// GETs a signed path. The signature is the permission, so the token
    /// is not sent.
    async fn signed(&self, path: &str) -> Result<reqwest::Response> {
        send_patiently(self.http.get(self.absolute(path))).await
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
        let resp = send_patiently(req).await?;
        Ok(resp.json().await?)
    }

    async fn send_empty(req: RequestBuilder) -> Result<()> {
        send_patiently(req).await?;
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

    /// Starts a sign-in with the server's identity provider. The browser
    /// goes to `url`; the provider's answer comes back to `return_to` as an
    /// `oidc_code` (or `oidc_error`) query parameter, which
    /// [`Self::oidc_finish`] redeems together with the `handle`.
    pub async fn oidc_begin(&self, return_to: &str) -> Result<OidcStart> {
        let body = OidcBegin {
            return_to: return_to.to_owned(),
        };
        self.json(Method::POST, "/auth/oidc/begin", &body).await
    }

    /// Like [`Self::oidc_begin`], to add the provider sign-in to the
    /// signed-in account with [`Self::oidc_link`].
    pub async fn oidc_begin_link(&self, return_to: &str) -> Result<OidcStart> {
        let body = OidcBegin {
            return_to: return_to.to_owned(),
        };
        self.json(Method::POST, "/auth/oidc/begin-link", &body)
            .await
    }

    pub async fn oidc_finish(&self, finish: &OidcFinish) -> Result<AuthResponse> {
        self.json(Method::POST, "/auth/oidc/finish", finish).await
    }

    pub async fn oidc_link(&self, finish: &OidcFinish) -> Result<Identity> {
        self.json(Method::POST, "/auth/oidc/link", finish).await
    }

    /// Whether the account has a password, and its provider sign-ins.
    pub async fn sign_in_methods(&self) -> Result<SignInMethods> {
        Self::send(self.request(Method::GET, "/auth/identities")?).await
    }

    pub async fn unlink_identity(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::DELETE, &format!("/auth/identities/{id}"))?).await
    }

    /// Needs the current password, if the account has one; signs out every
    /// other session.
    pub async fn change_password(&self, current: &str, new: &str) -> Result<()> {
        let body = PasswordChange {
            current: current.to_owned(),
            new: new.to_owned(),
        };
        Self::send_empty(self.request(Method::PUT, "/auth/password")?.json(&body)).await
    }

    pub async fn sessions(&self) -> Result<Vec<SessionInfo>> {
        Self::send(self.request(Method::GET, "/auth/sessions")?).await
    }

    pub async fn revoke_session(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::DELETE, &format!("/auth/sessions/{id}"))?).await
    }

    /// Signs out everywhere but here.
    pub async fn revoke_other_sessions(&self) -> Result<RevokeResult> {
        Self::send(self.request(Method::DELETE, "/auth/sessions")?).await
    }

    /// The caller's API keys. Needs a sign-in session, not a key.
    pub async fn api_keys(&self) -> Result<Vec<ApiKey>> {
        Self::send(self.request(Method::GET, "/keys")?).await
    }

    /// The answer holds the only copy of the new key's token.
    pub async fn create_api_key(&self, key: &NewApiKey) -> Result<CreatedApiKey> {
        self.json(Method::POST, "/keys", key).await
    }

    pub async fn delete_api_key(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::DELETE, &format!("/keys/{id}"))?).await
    }

    /// The caller's share links, newest first.
    pub async fn shares(&self) -> Result<Vec<Share>> {
        Self::send(self.request(Method::GET, "/shares")?).await
    }

    pub async fn create_share(&self, share: &NewShare) -> Result<Share> {
        self.json(Method::POST, "/shares", share).await
    }

    /// Stops the link working at once.
    pub async fn delete_share(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::DELETE, &format!("/shares/{id}"))?).await
    }

    /// What a share link shows; needs no account.
    pub async fn shared(&self, token: &str) -> Result<SharedView> {
        Self::send(self.request(Method::GET, &format!("/shared/{}", segment(token)))?).await
    }

    pub async fn me(&self) -> Result<User> {
        Self::send(self.request(Method::GET, "/auth/me")?).await
    }

    /// What the signed-in account has uploaded, against its storage limit.
    pub async fn usage(&self) -> Result<Usage> {
        Self::send(self.request(Method::GET, "/auth/usage")?).await
    }

    pub async fn admin_stats(&self) -> Result<ServerStats> {
        Self::send(self.request(Method::GET, "/admin/stats")?).await
    }

    pub async fn admin_settings(&self) -> Result<ServerSettings> {
        Self::send(self.request(Method::GET, "/admin/settings")?).await
    }

    pub async fn set_admin_settings(&self, settings: &ServerSettings) -> Result<ServerSettings> {
        self.json(Method::PUT, "/admin/settings", settings).await
    }

    pub async fn admin_users(&self) -> Result<Vec<AdminUser>> {
        Self::send(self.request(Method::GET, "/admin/users")?).await
    }

    pub async fn update_user(&self, id: i64, update: &UserUpdate) -> Result<AdminUser> {
        self.json(Method::PATCH, &format!("/admin/users/{id}"), update)
            .await
    }

    /// Signs that account out everywhere.
    pub async fn reset_user_password(&self, id: i64, new: &str) -> Result<()> {
        let body = PasswordReset {
            new: new.to_owned(),
        };
        let req = self
            .request(Method::POST, &format!("/admin/users/{id}/password"))?
            .json(&body);
        Self::send_empty(req).await
    }

    pub async fn delete_user(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::DELETE, &format!("/admin/users/{id}"))?).await
    }

    /// The database backups, with signed download links (see [`Self::absolute`]).
    pub async fn backups(&self) -> Result<Backups> {
        Self::send(self.request(Method::GET, "/admin/backups")?).await
    }

    /// Answers once the backup is written, which takes a while on a large
    /// collection.
    pub async fn create_backup(&self) -> Result<Backup> {
        Self::send(self.request(Method::POST, "/admin/backups")?).await
    }

    pub async fn set_backup_schedule(&self, schedule: &BackupSchedule) -> Result<BackupSchedule> {
        self.json(Method::PUT, "/admin/backups/schedule", schedule)
            .await
    }

    pub async fn delete_backup(&self, name: &str) -> Result<()> {
        let path = format!("/admin/backups/{}", segment(name));
        Self::send_empty(self.request(Method::DELETE, &path)?).await
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

    /// The posts before and after `id` in the results of search `query`.
    pub async fn neighbors(&self, id: i64, query: &str) -> Result<Neighbors> {
        let req = self
            .request(Method::GET, &format!("/posts/{id}/neighbors"))?
            .query(&[("q", query)]);
        Self::send(req).await
    }

    /// Up to `limit` posts like `id`, most alike first.
    pub async fn related(&self, id: i64, limit: u32) -> Result<RelatedPosts> {
        let req = self
            .request(Method::GET, &format!("/posts/{id}/related"))?
            .query(&[("limit", limit)]);
        Self::send(req).await
    }

    pub async fn update_post(&self, id: i64, update: &PostUpdate) -> Result<Post> {
        self.json(Method::PATCH, &format!("/posts/{id}"), update)
            .await
    }

    /// Tags, rating or deletion for many posts at once.
    pub async fn bulk_edit(&self, edit: &BulkEdit) -> Result<BulkResult> {
        self.json(Method::POST, "/posts/bulk", edit).await
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

    /// Saves a turned, flipped, cropped or trimmed copy as a new post.
    pub async fn edit_post(&self, id: i64, edit: &MediaEdit) -> Result<Post> {
        self.json(Method::POST, &format!("/posts/{id}/edit"), edit)
            .await
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

    /// Asks the server to download `request.url` and add it as a post.
    pub async fn import_url(&self, request: &ImportRequest) -> Result<Post> {
        self.json(Method::POST, "/posts/import", request).await
    }

    /// One page of a search on another booru, fetched by the server. Import
    /// a result with [`Client::import_url`].
    pub async fn remote_search(&self, search: &RemoteSearch) -> Result<RemotePage> {
        self.json(Method::POST, "/remote/search", search).await
    }

    /// The signed-in user's saved searches, by name.
    pub async fn saved_searches(&self) -> Result<Vec<SavedSearch>> {
        Self::send(self.request(Method::GET, "/searches")?).await
    }

    pub async fn save_search(&self, name: &str, query: &str) -> Result<SavedSearch> {
        let body = SaveSearchRequest {
            name: name.to_owned(),
            query: query.to_owned(),
        };
        self.json(Method::POST, "/searches", &body).await
    }

    pub async fn delete_search(&self, id: i64) -> Result<()> {
        Self::send_empty(self.request(Method::DELETE, &format!("/searches/{id}"))?).await
    }

    /// Trending posts and tags over the last `days`, narrowed by `query`.
    pub async fn explore(&self, days: u32, query: &str) -> Result<Explore> {
        let req = self
            .request(Method::GET, "/explore")?
            .query(&[("days", days.to_string()), ("q", query.to_owned())]);
        Self::send(req).await
    }

    /// Groups of posts that look alike, within `distance` bits.
    pub async fn duplicates(&self, distance: u32, limit: u32) -> Result<Duplicates> {
        let req = self
            .request(Method::GET, "/duplicates")?
            .query(&[("distance", distance), ("limit", limit)]);
        Self::send(req).await
    }

    /// Keeps one post of a group and moves the others to the recycle bin.
    pub async fn resolve_duplicates(&self, req: &ResolveDuplicates) -> Result<Post> {
        self.json(Method::POST, "/duplicates/resolve", req).await
    }

    /// Marks posts as not duplicates of each other.
    pub async fn dismiss_duplicates(&self, ids: Vec<i64>) -> Result<()> {
        Self::send_empty(
            self.request(Method::POST, "/duplicates/dismiss")?
                .json(&DismissDuplicates { ids }),
        )
        .await
    }

    /// Tag autocomplete: tags starting with `prefix`, most used first.
    pub async fn tags(&self, prefix: &str, limit: u32) -> Result<Vec<Tag>> {
        let req = self
            .request(Method::GET, "/tags")?
            .query(&[("q", prefix.to_owned()), ("limit", limit.to_string())]);
        Self::send(req).await
    }

    /// One page of the tag list, optionally filtered by a name prefix.
    pub async fn tag_list(
        &self,
        prefix: &str,
        order: TagOrder,
        page: u32,
        limit: u32,
    ) -> Result<Vec<Tag>> {
        let order = match order {
            TagOrder::Count => "count",
            TagOrder::Name => "name",
        };
        let req = self.request(Method::GET, "/tags")?.query(&[
            ("q", prefix.to_owned()),
            ("order", order.to_owned()),
            ("page", page.to_string()),
            ("limit", limit.to_string()),
        ]);
        Self::send(req).await
    }

    /// A tag with its aliases and implications. An alias resolves to its tag.
    pub async fn tag(&self, name: &str) -> Result<TagDetail> {
        Self::send(self.request(Method::GET, &format!("/tags/{}", segment(name)))?).await
    }

    pub async fn update_tag(&self, name: &str, update: &TagUpdate) -> Result<Tag> {
        self.json(Method::PATCH, &format!("/tags/{}", segment(name)), update)
            .await
    }

    /// Admins: makes `alias` another name for the tag, merging an existing
    /// tag of that name into it.
    pub async fn add_alias(&self, name: &str, alias: &str) -> Result<TagDetail> {
        let body = AliasRequest {
            alias: alias.to_owned(),
        };
        self.json(
            Method::POST,
            &format!("/tags/{}/aliases", segment(name)),
            &body,
        )
        .await
    }

    pub async fn remove_alias(&self, name: &str, alias: &str) -> Result<TagDetail> {
        let path = format!("/tags/{}/aliases/{}", segment(name), segment(alias));
        Self::send(self.request(Method::DELETE, &path)?).await
    }

    /// Admins: every post tagged `name` also gets `implies`.
    pub async fn add_implication(&self, name: &str, implies: &str) -> Result<TagDetail> {
        let body = ImplicationRequest {
            implies: implies.to_owned(),
        };
        let path = format!("/tags/{}/implications", segment(name));
        self.json(Method::POST, &path, &body).await
    }

    pub async fn remove_implication(&self, name: &str, implied: &str) -> Result<TagDetail> {
        let path = format!("/tags/{}/implications/{}", segment(name), segment(implied));
        Self::send(self.request(Method::DELETE, &path)?).await
    }
}

/// Percent-encodes one URL path segment.
fn segment(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// Sends `req`, and on a 429 waits as the server asks and sends it again
/// (a rate-limited request wasn't processed, so this is safe for any
/// method). Requests whose body can't be replayed, like streamed uploads,
/// are sent once.
async fn send_patiently(req: RequestBuilder) -> Result<reqwest::Response> {
    let mut attempt = 0;
    let mut next = req;
    loop {
        let spare = next.try_clone();
        match check(next.send().await?).await {
            Err(ClientError::RateLimited { retry_after }) => {
                let wait = retry_delay(attempt, retry_after);
                let (Some(wait), Some(again)) = (wait, spare) else {
                    return Err(ClientError::RateLimited { retry_after });
                };
                sleep(wait).await;
                attempt += 1;
                next = again;
            }
            other => return other,
        }
    }
}

/// How long to wait before retry number `attempt + 1`, or `None` to give
/// up. Honours `Retry-After` (seconds) up to [`MAX_RETRY_WAIT`]; without
/// it, backs off 1 s, 2 s, …
fn retry_delay(attempt: u32, retry_after: Option<u64>) -> Option<Duration> {
    if attempt >= RATE_LIMIT_RETRIES {
        return None;
    }
    let wait = match retry_after {
        Some(secs) => Duration::from_secs(secs),
        None => Duration::from_secs(1u64 << attempt.min(5)),
    };
    (wait <= MAX_RETRY_WAIT).then_some(wait)
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep(wait: Duration) {
    tokio::time::sleep(wait).await;
}

#[cfg(target_arch = "wasm32")]
async fn sleep(wait: Duration) {
    let ms = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
    gloo_timers::future::TimeoutFuture::new(ms).await;
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

    #[test]
    fn plans_rate_limit_retries() {
        assert_eq!(retry_delay(0, None), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay(1, None), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(2, None), None);
        assert_eq!(retry_delay(0, Some(0)), Some(Duration::ZERO));
        assert_eq!(retry_delay(1, Some(30)), Some(MAX_RETRY_WAIT));
        assert_eq!(retry_delay(0, Some(31)), None);
        assert_eq!(retry_delay(u32::MAX, Some(1)), None);
        let limited = |retry_after| ClientError::RateLimited { retry_after }.to_string();
        assert_eq!(limited(None), "too many requests, try again later");
        assert_eq!(
            limited(Some(30)),
            "too many attempts, try again in a minute"
        );
        assert_eq!(
            limited(Some(840)),
            "too many attempts, try again in 14 minutes"
        );
    }

    /// Serves canned HTTP responses, one per connection, and counts them.
    async fn canned(responses: Vec<String>) -> (String, tokio::task::JoinHandle<usize>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut served = 0;
            for body in responses {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await.unwrap();
                sock.write_all(body.as_bytes()).await.unwrap();
                sock.shutdown().await.unwrap();
                served += 1;
            }
            served
        });
        (url, task)
    }

    const LIMITED: &str = "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
    const LIMITED_LONG: &str = "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 3600\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
    fn ok_info() -> String {
        let body = serde_json::to_string(&ServerInfo {
            name: "StarBooru".into(),
            version: "1".into(),
            has_users: true,
            registration_open: false,
            max_upload_bytes: 1,
            url_import: false,
            oidc: None,
            video_editing: false,
        })
        .unwrap();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn retries_after_rate_limit() {
        let (url, server) = canned(vec![LIMITED.into(), LIMITED.into(), ok_info()]).await;
        let info = Client::new(&url).unwrap().info().await.unwrap();
        assert_eq!(info.name, "StarBooru");
        assert_eq!(server.await.unwrap(), 3);
    }

    #[tokio::test]
    async fn gives_up_when_still_limited_or_asked_to_wait_long() {
        let (url, server) = canned(vec![LIMITED.into(); 3]).await;
        let err = Client::new(&url).unwrap().info().await.unwrap_err();
        assert!(matches!(
            err,
            ClientError::RateLimited {
                retry_after: Some(0)
            }
        ));
        assert_eq!(server.await.unwrap(), 3);

        let (url, server) = canned(vec![LIMITED_LONG.into()]).await;
        let err = Client::new(&url).unwrap().info().await.unwrap_err();
        assert!(matches!(
            err,
            ClientError::RateLimited {
                retry_after: Some(3600)
            }
        ));
        assert_eq!(server.await.unwrap(), 1);
    }

    fn media_response(body: &str, length: usize) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nConnection: close\r\nContent-Length: {length}\r\n\r\n{body}"
        )
    }

    #[tokio::test]
    async fn downloads_media_up_to_a_limit() {
        let (url, server) = canned(vec![media_response("jpeg!", 5)]).await;
        let client = Client::new(&url).unwrap().with_token(Some("secret".into()));
        let bytes = client
            .media_bytes("/media/thumb/ab.jpg?exp=1&sig=2", 5)
            .await
            .unwrap();
        assert_eq!(bytes, b"jpeg!");
        assert_eq!(server.await.unwrap(), 1);

        // Refused from the announced length, before reading the body.
        let (url, _server) = canned(vec![media_response("jpeg!", 5)]).await;
        let err = Client::new(&url)
            .unwrap()
            .media_bytes("/media/thumb/ab.jpg", 4)
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::TooLarge { limit: 4 }));

        // Only media paths, so it can't be pointed at the API.
        let err = client.media_bytes("/api/v1/posts", 5).await.unwrap_err();
        assert!(matches!(err, ClientError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn downloads_to_a_file() {
        let dir = std::env::temp_dir().join(format!("sb-client-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("a.dump");

        let (url, _server) = canned(vec![media_response("backup", 6)]).await;
        let client = Client::new(&url).unwrap();
        let size = client
            .download_to("/backups/a.dump?exp=1&sig=2", &dest)
            .await
            .unwrap();
        assert_eq!(size, 6);
        assert_eq!(std::fs::read(&dest).unwrap(), b"backup");
        assert!(!dir.join("a.dump.part").exists());

        // A failed download leaves neither the file nor its part behind.
        let (url, _server) = canned(vec![LIMITED_LONG.into()]).await;
        let other = dir.join("b.dump");
        let err = Client::new(&url)
            .unwrap()
            .download_to("/backups/b.dump", &other)
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::RateLimited { .. }));
        assert!(!other.exists());

        // Only signed downloads, never the API.
        let err = client
            .download_to("/api/v1/posts", &other)
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::InvalidUrl(_)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn encodes_path_segments() {
        assert_eq!(segment("blue_sky"), "blue_sky");
        assert_eq!(segment("a/b?c#d"), "a%2Fb%3Fc%23d");
        assert_eq!(segment("c++ x"), "c%2B%2B%20x");
        assert_eq!(segment("città"), "citt%C3%A0");
    }
}
