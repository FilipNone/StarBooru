//! A small client for an OpenAI-compatible chat API (OpenAI, OpenRouter,
//! Gemini's OpenAI endpoint, or a local Ollama / LM Studio / vLLM), and the
//! AI features built on it: tag suggestions and auto-tagging of uploads.
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
use starbooru_core::tag::normalize_tags;
use starbooru_core::{TagCategory, parse_suggestions};
use tokio::sync::Semaphore;
use tracing::warn;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::{Storage, Variant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest a whole answer may take; vision models on a CPU are slow.
const TIMEOUT: Duration = Duration::from_secs(60);
/// Most bytes read from an answer.
const MAX_ANSWER_BYTES: usize = 1 << 20;
/// Largest image sent.
const MAX_IMAGE_BYTES: usize = 8 << 20;
const USER_AGENT: &str = concat!("StarBooru/", env!("CARGO_PKG_VERSION"));

/// Auto-tagging runs at most this many requests at once.
static AUTOTAG_SLOTS: Semaphore = Semaphore::const_new(2);

/// The provider in force (see `settings::ai`).
#[derive(Clone, Default)]
pub struct Provider {
    /// Base URL, without a trailing slash. Empty means AI is off.
    pub url: String,
    pub key: String,
    pub model: String,
    pub autotag: bool,
}

impl Provider {
    pub fn enabled(&self) -> bool {
        !self.url.is_empty() && !self.model.is_empty()
    }
}

fn client() -> AppResult<&'static reqwest::Client> {
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

fn request_error(e: &reqwest::Error) -> AppError {
    if e.is_timeout() {
        AppError::Upstream("the AI provider took too long to answer".into())
    } else {
        AppError::Upstream("can't reach the AI provider".into())
    }
}

/// Sends one user message, with an optional JPEG, to `{url}/chat/completions`
/// and returns the text of the first choice.
pub async fn chat(provider: &Provider, prompt: &str, jpeg: Option<&[u8]>) -> AppResult<String> {
    if !provider.enabled() {
        return Err(AppError::BadRequest(
            "AI features are turned off on this server".into(),
        ));
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
        .body(body);
    if !provider.key.is_empty() {
        req = req.bearer_auth(&provider.key);
    }
    let mut resp = req.send().await.map_err(|e| request_error(&e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::Upstream(format!(
            "AI provider answered {}",
            status.as_u16()
        )));
    }
    let mut answer = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| request_error(&e))? {
        if answer.len().saturating_add(chunk.len()) > MAX_ANSWER_BYTES {
            return Err(AppError::Upstream(
                "the AI provider's answer is too long".into(),
            ));
        }
        answer.extend_from_slice(&chunk);
    }
    answer_text(&answer)
        .ok_or_else(|| AppError::Upstream("the AI provider's answer is not understood".into()))
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
    let provider = crate::settings::ai(state).await?;
    if !provider.enabled() || !provider.autotag {
        return Ok(());
    }
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
}
