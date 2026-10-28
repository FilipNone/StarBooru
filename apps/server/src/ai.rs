//! A small client for an OpenAI-compatible chat API (OpenAI, OpenRouter,
//! Gemini's OpenAI endpoint, or a local Ollama / LM Studio / vLLM), and the
//! AI features built on it: tag suggestions and auto-tagging of uploads.
//! Each tool runs on the hosted provider, the local runtime
//! (`crate::local_ai`) or nowhere, as the admin picks.
//!
//! The provider is set by the admin and may sit on the LAN, so unlike URL
//! import any address is allowed. Proxies from the environment are ignored,
//! the answer is capped while it streams, and slow providers time out. The
//! API key is never logged or returned to clients; neither are images or
//! answers.

use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};
use starbooru_core::ai::{ParsedSuggestions, TAG_PROMPT};
use starbooru_core::local_ai::machine_tier;
use starbooru_core::tag::normalize_tags;
use starbooru_core::{AiRoute, TagCategory, parse_suggestions};
use tokio::sync::Semaphore;
use tracing::warn;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::{Storage, Variant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest a whole answer from a hosted provider may take.
const TIMEOUT: Duration = Duration::from_secs(60);
/// Longest a local answer may take: a Raspberry Pi needs a minute or more.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(300);
/// Most bytes read from an answer.
const MAX_ANSWER_BYTES: usize = 1 << 20;
/// Largest image sent.
const MAX_IMAGE_BYTES: usize = 8 << 20;
const USER_AGENT: &str = concat!("StarBooru/", env!("CARGO_PKG_VERSION"));

/// Auto-tagging runs at most this many requests at once.
static AUTOTAG_SLOTS: Semaphore = Semaphore::const_new(2);

/// Where one request goes.
#[derive(Clone, Default)]
pub struct Provider {
    /// Base URL of the OpenAI-compatible API, without a trailing slash.
    /// Empty means off.
    pub url: String,
    pub key: String,
    pub model: String,
    pub timeout: Duration,
    /// The local runtime, for error messages.
    pub local: bool,
}

impl Provider {
    pub fn hosted(url: String, key: String, model: String) -> Self {
        Self {
            url,
            key,
            model,
            timeout: TIMEOUT,
            local: false,
        }
    }

    pub fn enabled(&self) -> bool {
        !self.url.is_empty() && !self.model.is_empty()
    }
}

/// The AI setup in force (see `settings::ai`).
#[derive(Clone, Default)]
pub struct AiSetup {
    pub hosted: Provider,
    pub autotag: bool,
    pub local_enabled: bool,
    /// Ollama's base address, without `/v1`.
    pub local_url: String,
    /// Empty: the preset recommended for this machine.
    pub local_model: String,
    pub tagging: AiRoute,
    pub translation: AiRoute,
}

impl AiSetup {
    /// The local model: the admin's pick or the recommended preset.
    pub fn local_model(&self) -> String {
        if self.local_model.is_empty() {
            let hw = crate::local_ai::hardware();
            machine_tier(&hw).preset().model.to_owned()
        } else {
            self.local_model.clone()
        }
    }

    pub fn local(&self) -> Provider {
        Provider {
            url: format!("{}/v1", self.local_url),
            key: String::new(),
            model: self.local_model(),
            timeout: LOCAL_TIMEOUT,
            local: true,
        }
    }

    fn route(&self, route: AiRoute) -> Option<Provider> {
        let provider = match route {
            AiRoute::Off => return None,
            AiRoute::Hosted => self.hosted.clone(),
            AiRoute::Local if self.local_enabled && !self.local_url.is_empty() => self.local(),
            AiRoute::Local => return None,
        };
        provider.enabled().then_some(provider)
    }

    /// Where tag suggestions go, if anywhere.
    pub fn tagging(&self) -> Option<Provider> {
        self.route(self.tagging)
    }

    /// Where translations go, if anywhere.
    pub fn translation(&self) -> Option<Provider> {
        self.route(self.translation)
    }
}

/// The error for a tool that is off.
pub fn turned_off() -> AppError {
    AppError::BadRequest("this AI feature is turned off on this server".into())
}

pub(crate) fn client() -> AppResult<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // sqlx already uses ring; Err means a provider is installed already.
            let _ = rustls::crypto::ring::default_provider().install_default();
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .no_proxy()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(TIMEOUT)
                .build()
                .map_err(|e| warn!("cannot start the AI HTTP client: {e}"))
                .ok()
        })
        .as_ref()
        .ok_or_else(|| AppError::Internal("the AI HTTP client failed to start".into()))
}

pub(crate) fn request_error(e: &reqwest::Error, local: bool) -> AppError {
    let who = if local {
        "the local AI runtime"
    } else {
        "the AI provider"
    };
    if e.is_timeout() {
        AppError::Upstream(format!("{who} took too long to answer"))
    } else {
        AppError::Upstream(format!("can't reach {who}"))
    }
}

/// Sends one user message, with an optional JPEG, to `{url}/chat/completions`
/// and returns the text of the first choice.
pub async fn chat(provider: &Provider, prompt: &str, jpeg: Option<&[u8]>) -> AppResult<String> {
    if !provider.enabled() {
        return Err(turned_off());
    }
    let content = match jpeg {
        None => json!(prompt),
        Some(bytes) => {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            json!([
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": { "url": format!("data:image/jpeg;base64,{data}") } },
            ])
        }
    };
    let body = json!({
        "model": provider.model,
        "messages": [{ "role": "user", "content": content }],
        "temperature": 0.2,
    });
    let body = serde_json::to_vec(&body).map_err(AppError::internal)?;
    let mut req = client()?
        .post(format!("{}/chat/completions", provider.url))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .timeout(provider.timeout)
        .body(body);
    if !provider.key.is_empty() {
        req = req.bearer_auth(&provider.key);
    }
    let local = provider.local;
    let mut resp = req.send().await.map_err(|e| request_error(&e, local))?;
    let status = resp.status();
    if !status.is_success() {
        // The provider's own reason ("rate limited", "no credits") is what
        // the admin can act on. It can't hold the key: that went out only
        // in the header.
        let reason = read_capped(&mut resp, local)
            .await
            .ok()
            .and_then(|b| error_reason(&b));
        let who = if local {
            "local AI runtime"
        } else {
            "AI provider"
        };
        let code = status.as_u16();
        return Err(AppError::Upstream(match reason {
            Some(r) => format!("{who} answered {code}: {r}"),
            None => format!("{who} answered {code}"),
        }));
    }
    let answer = read_capped(&mut resp, local).await?;
    answer_text(&answer)
        .ok_or_else(|| AppError::Upstream("the AI provider's answer is not understood".into()))
}

/// Reads a whole answer, at most `MAX_ANSWER_BYTES`.
async fn read_capped(resp: &mut reqwest::Response, local: bool) -> AppResult<Vec<u8>> {
    let mut answer = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| request_error(&e, local))? {
        if answer.len().saturating_add(chunk.len()) > MAX_ANSWER_BYTES {
            return Err(AppError::Upstream(
                "the AI provider's answer is too long".into(),
            ));
        }
        answer.extend_from_slice(&chunk);
    }
    Ok(answer)
}

/// The text of the first choice of a chat completion.
fn answer_text(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let content = value
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?;
    match content {
        Value::String(s) => Some(s.clone()),
        // Some providers answer with parts, like the request.
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(""),
        ),
        _ => None,
    }
}

/// Most characters of a provider's error message passed on.
const MAX_REASON_CHARS: usize = 200;

/// The message of an OpenAI-style error body (`{"error": {"message": …}}`),
/// on one line and capped. OpenRouter puts the upstream's reason in
/// `metadata.raw`; it is used when the message alone is generic.
fn error_reason(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let error = value.get("error")?;
    let message = match error {
        Value::String(s) => s.as_str(),
        _ => error.get("message").and_then(Value::as_str)?,
    };
    let raw = error
        .get("metadata")
        .and_then(|m| m.get("raw"))
        .and_then(Value::as_str)
        .filter(|r| !r.trim().is_empty());
    let text = match raw {
        Some(raw) if message.len() < 80 => format!("{message} ({raw})"),
        _ => message.to_owned(),
    };
    let text: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_REASON_CHARS)
        .collect();
    (!text.is_empty()).then_some(text)
}

/// The post's thumbnail (a JPEG), for showing to a model.
async fn thumbnail(state: &AppState, post_id: i64) -> AppResult<Vec<u8>> {
    let row: Option<(Vec<u8>, String, bool)> = sqlx::query_as(
        "SELECT sha256, ext, has_thumb FROM posts WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await?;
    let (sha, ext, has_thumb) = row.ok_or(AppError::NotFound)?;
    if !has_thumb {
        return Err(AppError::BadRequest(
            "this post has no thumbnail to show the AI".into(),
        ));
    }
    let sha = hex::encode(sha);
    let path = state
        .storage
        .path_for_file(
            Variant::Thumb,
            &Storage::file_name(Variant::Thumb, &sha, &ext),
        )
        .ok_or_else(|| AppError::Internal("bad thumbnail name".into()))?;
    let bytes = tokio::fs::read(&path).await.map_err(AppError::internal)?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::BadRequest("the thumbnail is too large".into()));
    }
    Ok(bytes)
}

/// Asks the provider for tags for a post. Changes nothing.
pub async fn suggest(
    state: &AppState,
    provider: &Provider,
    post_id: i64,
) -> AppResult<ParsedSuggestions> {
    let jpeg = thumbnail(state, post_id).await?;
    let answer = chat(provider, TAG_PROMPT, Some(&jpeg)).await?;
    Ok(parse_suggestions(&answer))
}

/// Starts auto-tagging a new post in the background when it is turned on.
/// Failures are logged without content and never affect the upload.
pub fn autotag_later(state: &AppState, post_id: i64) {
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = autotag(&state, post_id).await {
            warn!(post_id, "automatic tagging failed: {e}");
        }
    });
}

async fn autotag(state: &AppState, post_id: i64) -> AppResult<()> {
    let setup = crate::settings::ai(state).await?;
    let Some(provider) = setup.tagging().filter(|_| setup.autotag) else {
        return Ok(());
    };
    let _slot = AUTOTAG_SLOTS
        .acquire()
        .await
        .map_err(|_| AppError::Internal("auto-tagging is shutting down".into()))?;
    let parsed = suggest(state, &provider, post_id).await?;
    add_tags(state, post_id, &parsed).await
}

/// Adds suggested tags to a post through the normal tag path. New tags get
/// the category the model gave, or general.
pub async fn add_tags(state: &AppState, post_id: i64, parsed: &ParsedSuggestions) -> AppResult<()> {
    if parsed.tags.is_empty() {
        return Ok(());
    }
    let mut tx = state.db.begin().await?;
    let current: Option<Vec<String>> = sqlx::query_scalar(
        "SELECT COALESCE((SELECT array_agg(t.name) FROM tags t WHERE t.id = ANY(p.tag_ids)), \
         '{}') FROM posts p WHERE p.id = $1 AND p.deleted_at IS NULL FOR UPDATE",
    )
    .bind(post_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(mut names) = current else {
        return Ok(()); // deleted meanwhile
    };
    for (name, category) in &parsed.tags {
        // New tags only: an existing tag keeps its category.
        if let Some(cat) = category.filter(|c| *c != TagCategory::General) {
            sqlx::query(
                "INSERT INTO tags (name, category) SELECT $1, $2 \
                 WHERE NOT EXISTS (SELECT 1 FROM tag_aliases WHERE alias = $1) \
                 ON CONFLICT (name) DO NOTHING",
            )
            .bind(name)
            .bind(cat.to_db())
            .execute(&mut *tx)
            .await?;
        }
        names.push(name.clone());
    }
    let names = normalize_tags(names.iter().map(String::as_str))?;
    crate::routes::set_post_tags(&mut tx, post_id, &names).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_answers() {
        let body = br#"{"choices":[{"message":{"role":"assistant","content":"hi"}}]}"#;
        assert_eq!(answer_text(body).as_deref(), Some("hi"));
        let parts =
            br#"{"choices":[{"message":{"content":[{"type":"text","text":"a"},{"text":"b"}]}}]}"#;
        assert_eq!(answer_text(parts).as_deref(), Some("ab"));
        assert_eq!(answer_text(b"{}"), None);
        assert_eq!(answer_text(b"not json"), None);
    }

    #[test]
    fn reads_error_reasons() {
        let openai = br#"{"error":{"message":"Incorrect API key\nprovided","type":"invalid"}}"#;
        assert_eq!(
            error_reason(openai).as_deref(),
            Some("Incorrect API key provided")
        );
        let openrouter = br#"{"error":{"message":"Provider returned error","code":429,
            "metadata":{"raw":"qwen is temporarily rate-limited upstream"}}}"#;
        assert_eq!(
            error_reason(openrouter).as_deref(),
            Some("Provider returned error (qwen is temporarily rate-limited upstream)")
        );
        assert_eq!(
            error_reason(br#"{"error":"busy"}"#).as_deref(),
            Some("busy")
        );
        let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(500));
        assert_eq!(error_reason(long.as_bytes()).map(|r| r.len()), Some(200));
        assert_eq!(error_reason(b"<html>bad gateway</html>"), None);
        assert_eq!(error_reason(br#"{"error":{"message":"  "}}"#), None);
    }
}
