//! Translated text over images: comic and manga panels.
//!
//! The image (fit in `PAGE_SIZE`², JPEG) goes to the translation route's
//! vision model, which answers with each piece of text, its box and its
//! translation. Unlike text translation, the image itself is sent, so a
//! hosted provider sees it; local AI keeps it on the server. The result is
//! kept per post and language in `post_translations`. Never log images,
//! text or translations.

use std::io::Cursor;

use starbooru_core::{PanelTranslation, TextBlock, panel_prompt, parse_panel_blocks};
use tokio::sync::Semaphore;
use tracing::warn;

use crate::ai::{self, Provider};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::{self, FilterKind};

/// Longest side of the image sent, in pixels: big enough to read balloon
/// text, small enough for a local model's context.
const PAGE_SIZE: u32 = 1536;
/// One page at a time: a model reading a whole page is slow, and a burst of
/// uploads must not queue many paid requests at once.
static SLOTS: Semaphore = Semaphore::const_new(1);

/// sha256, ext, kind, has_sample.
type PageRow = (Vec<u8>, String, i16, Option<bool>);

/// The post's still image as a JPEG for the model, with its size.
async fn page_jpeg(state: &AppState, post_id: i64) -> AppResult<(Vec<u8>, u32, u32)> {
    let row: Option<PageRow> = sqlx::query_as(
        "SELECT sha256, ext, kind, has_sample FROM posts WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await?;
    let (sha, ext, kind, has_sample) = row.ok_or(AppError::NotFound)?;
    if kind != 0 {
        return Err(AppError::BadRequest(
            "only still images can be translated".into(),
        ));
    }
    let path = crate::ocr::source_path(&state.storage, &hex::encode(sha), &ext, has_sample)
        .ok_or_else(|| AppError::BadRequest("this image's format can't be translated".into()))?;
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(AppError::internal)?;
        let img = storage::decode(&bytes)?;
        let jpeg = storage::fit_jpeg(&img, PAGE_SIZE, FilterKind::Sharp)?;
        let (w, h) = image::ImageReader::new(Cursor::new(&jpeg))
            .with_guessed_format()
            .map_err(AppError::internal)?
            .into_dimensions()
            .map_err(AppError::internal)?;
        Ok((jpeg, w, h))
    })
    .await
    .map_err(AppError::internal)?
}

/// Reads and translates the text in a post's image into `lang` (already
/// checked) and keeps the result, replacing an older one. A result with no
/// text is returned but not kept.
pub async fn translate(
    state: &AppState,
    provider: &Provider,
    post_id: i64,
    lang: &str,
) -> AppResult<PanelTranslation> {
    let _slot = SLOTS
        .acquire()
        .await
        .map_err(|_| AppError::Internal("translation is shutting down".into()))?;
    let (jpeg, w, h) = page_jpeg(state, post_id).await?;
    let answer = ai::chat(provider, &panel_prompt(lang), Some(&jpeg)).await?;
    let blocks = parse_panel_blocks(&answer, w, h).ok_or_else(|| {
        AppError::Upstream("the AI's answer has no text blocks in it; try again".into())
    })?;
    let model = provider.model.clone();
    if blocks.is_empty() {
        // Nothing to draw. Not kept, so a flaky answer can't replace a
        // good translation.
        return Ok(PanelTranslation {
            lang: lang.to_owned(),
            blocks,
            model,
            created_at: chrono::Utc::now(),
        });
    }
    let json = serde_json::to_string(&blocks).map_err(AppError::internal)?;
    let created_at = sqlx::query_scalar(
        "INSERT INTO post_translations (post_id, lang, blocks, model) \
         VALUES ($1, $2, $3::jsonb, $4) \
         ON CONFLICT (post_id, lang) DO UPDATE SET blocks = $3::jsonb, model = $4, \
         created_at = now() \
         RETURNING created_at",
    )
    .bind(post_id)
    .bind(lang)
    .bind(&json)
    .bind(&model)
    .fetch_one(&state.db)
    .await?;
    Ok(PanelTranslation {
        lang: lang.to_owned(),
        blocks,
        model,
        created_at,
    })
}

/// The kept translations of a post, newest first.
pub async fn list(state: &AppState, post_id: i64) -> AppResult<Vec<PanelTranslation>> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT id FROM posts WHERE id = $1 AND deleted_at IS NULL")
            .bind(post_id)
            .fetch_optional(&state.db)
            .await?;
    found.ok_or(AppError::NotFound)?;
    let rows: Vec<(String, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT lang, blocks::text, model, created_at FROM post_translations \
         WHERE post_id = $1 ORDER BY created_at DESC",
    )
    .bind(post_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(lang, blocks, model, created_at)| PanelTranslation {
            lang,
            // Written by `translate`; unreadable rows show as empty.
            blocks: serde_json::from_str::<Vec<TextBlock>>(&blocks).unwrap_or_default(),
            model,
            created_at,
        })
        .collect())
}

/// Translates a new post in the background when OCR found text in it and
/// automatic translation is on. Failures are logged without content.
pub fn auto_translate_later(state: &AppState, post_id: i64) {
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = auto_translate(&state, post_id).await {
            warn!(post_id, "automatic translation failed: {e}");
        }
    });
}

pub(crate) async fn auto_translate(state: &AppState, post_id: i64) -> AppResult<()> {
    let setup = crate::settings::ai(state).await?;
    if setup.auto_translate.is_empty() {
        return Ok(());
    }
    let Some(provider) = setup.translation() else {
        return Ok(());
    };
    translate(state, &provider, post_id, &setup.auto_translate).await?;
    Ok(())
}
