//! `POST /posts/{id}/edit`: a turned, flipped, cropped or trimmed copy of a
//! post. Media never changes in place, so the copy is a new post with the
//! original's tags, rating and source.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use starbooru_core::{EditError, EditKind, MediaEdit, Post, edit_kind};

use super::posts;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

fn bad(e: EditError) -> AppError {
    AppError::BadRequest(e.to_string())
}

pub async fn edit(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<MediaEdit>,
) -> AppResult<Json<Post>> {
    let post = posts::load(&state, user.id, id).await?;
    if post.deleted {
        return Err(AppError::BadRequest(
            "restore the post from the recycle bin before editing it".into(),
        ));
    }
    let kind = edit_kind(post.kind, &post.mime).ok_or(bad(EditError::NotEditable))?;
    let (ext, uploader): (String, Option<i64>) =
        sqlx::query_as("SELECT ext, uploader_id FROM posts WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    // Checked first, so nothing is made that can't be finished.
    if req.bin_original && !user.can_moderate(uploader) {
        return Err(AppError::Forbidden);
    }
    let source = state
        .storage
        .original_path(&post.sha256, &ext)
        .ok_or_else(|| AppError::internal("bad storage path"))?;

    let max = state.config.max_upload_bytes;
    let worker = state.clone();
    let bytes = match kind {
        EditKind::Image => {
            let (mime, edit) = (post.mime.clone(), req.clone());
            tokio::task::spawn_blocking(move || {
                let original = std::fs::read(&source).map_err(AppError::internal)?;
                crate::edit::edit_image(&original, &mime, &edit)
            })
            .await??
        }
        EditKind::Video => {
            let width = post.width.and_then(|w| u32::try_from(w).ok()).unwrap_or(0);
            let height = post.height.and_then(|h| u32::try_from(h).ok()).unwrap_or(0);
            req.check(EditKind::Video, width, height, post.duration_ms)
                .map_err(bad)?;
            let trim = req.trim.ok_or(bad(EditError::NoChange))?;
            if !state.storage.has_tools() {
                return Err(AppError::BadRequest(
                    "trimming video needs ffmpeg on the server".into(),
                ));
            }
            tokio::task::spawn_blocking(move || {
                let tools = worker
                    .storage
                    .tools()
                    .ok_or_else(|| AppError::internal("ffmpeg went away"))?;
                let out = worker.storage.scratch_path(&ext);
                let done = tools.trim(&source, &out, &ext, trim.start_ms, trim.end_ms);
                let read = if done {
                    match std::fs::metadata(&out) {
                        Ok(m) if m.len() > max as u64 => Err(AppError::PayloadTooLarge),
                        Ok(_) => std::fs::read(&out).map_err(AppError::internal),
                        Err(e) => Err(AppError::internal(e)),
                    }
                } else {
                    Err(AppError::BadRequest(
                        "ffmpeg could not trim this video".into(),
                    ))
                };
                let _ = std::fs::remove_file(&out);
                read
            })
            .await??
        }
    };
    if bytes.len() > max {
        return Err(AppError::PayloadTooLarge);
    }

    let copy = posts::create_post(
        &state,
        user.id,
        Bytes::from(bytes),
        post.rating,
        post.source.clone(),
        &post.tags,
    )
    .await?;
    let figures = super::figures::load(&state.db, id).await?;
    if !figures.is_empty() {
        let mut tx = state.db.begin().await?;
        posts::lock_live(&mut tx, copy.id).await?;
        super::figures::replace(&mut tx, copy.id, figures).await?;
        tx.commit().await?;
    }
    if req.bin_original {
        posts::set_deleted(&state, &user, id, true).await?;
    }
    Ok(Json(posts::load_full(&state, user.id, copy.id).await?))
}

#[cfg(test)]
mod tests {
    use axum::http::Method;
    use serde_json::json;

    use crate::testing;

    #[tokio::test]
    async fn edits_make_a_new_post() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let bob = app.register("bob").await;
        // 12 × 8.
        let id = app.upload_png(&admin, 4, "sky blue_hair").await;
        let edit = |token: &str, id: i64, body: serde_json::Value| {
            let (app, token) = (&app, token.to_owned());
            async move {
                app.call(
                    Method::POST,
                    &format!("/api/v1/posts/{id}/edit"),
                    Some(&token),
                    Some(body),
                )
                .await
            }
        };

        for bad in [
            json!({}),
            json!({ "rotate": 45 }),
            json!({ "crop": { "x": 4, "y": 0, "width": 9, "height": 8 } }),
            json!({ "trim": { "start_ms": 0, "end_ms": 500 } }),
        ] {
            assert_eq!(edit(&admin, id, bad.clone()).await.status, 400, "{bad}");
        }
        assert_eq!(
            edit(&admin, 999_999, json!({ "rotate": 90 })).await.status,
            404
        );
        // Only the uploader or an admin may bin the original.
        let r = edit(&bob, id, json!({ "rotate": 90, "bin_original": true })).await;
        assert_eq!(r.status, 403);

        // A quarter turn, then the crop is in pixels of the turned image.
        let crop = json!({ "x": 1, "y": 2, "width": 6, "height": 5 });
        let r = edit(&admin, id, json!({ "rotate": 90, "crop": crop })).await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        let copy = r.json();
        assert_ne!(copy["id"].as_i64(), Some(id));
        assert_eq!(
            (copy["width"].as_i64(), copy["height"].as_i64()),
            (Some(6), Some(5))
        );
        assert_eq!(copy["mime"], "image/png");
        assert_eq!(copy["tags"], json!(["blue_hair", "sky"]));
        // The original stays as it was.
        let original = app.get(&format!("/api/v1/posts/{id}"), &admin).await.json();
        assert_eq!(original["width"].as_i64(), Some(12));
        assert_eq!(original["deleted"], false);

        // Anyone may make a copy; the uploader can also bin the original.
        let r = edit(&bob, id, json!({ "flip_h": true })).await;
        assert_eq!(r.status, 200);
        let r = edit(&admin, id, json!({ "flip_v": true, "bin_original": true })).await;
        assert_eq!(r.status, 200);
        let original = app.get(&format!("/api/v1/posts/{id}"), &admin).await.json();
        assert_eq!(original["deleted"], true);
        // Binned posts are restored before they are edited.
        assert_eq!(edit(&admin, id, json!({ "rotate": 90 })).await.status, 400);

        // The same edit twice is the same file: no second copy.
        let copy = copy["id"].as_i64().unwrap();
        let r = edit(&admin, copy, json!({ "rotate": 180 })).await;
        assert_eq!(r.status, 200);
        assert_eq!(
            edit(&admin, copy, json!({ "rotate": 180 })).await.status,
            409
        );
        app.finish().await;
    }

    #[tokio::test]
    async fn videos_are_trimmed_with_ffmpeg() {
        let Some(tools) = crate::probe::tests::tools() else {
            return;
        };
        let Some(app) = testing::app_with(|c| c.tools = Some(tools)).await else {
            return;
        };
        let t = app.register("admin").await;
        let src = tempfile::tempdir().unwrap();
        let Some(clip) = crate::probe::tests::generate(
            src.path(),
            "clip.webm",
            "testsrc=duration=4:size=64x48:rate=10",
            &["-c:v", "libvpx-vp9", "-g", "10", "-deadline", "realtime"],
        ) else {
            return app.finish().await;
        };
        let r = app.upload(&t, &clip, &[("tags", "clip")]).await.json();
        let id = r["id"].as_i64().unwrap();
        assert_eq!(r["duration_ms"], 4000);
        let path = format!("/api/v1/posts/{id}/edit");
        let edit = |body| app.call(Method::POST, &path, Some(&t), Some(body));

        assert_eq!(edit(json!({ "rotate": 90 })).await.status, 400);
        let past_the_end = json!({ "trim": { "start_ms": 1000, "end_ms": 4001 } });
        assert_eq!(edit(past_the_end).await.status, 400);
        let r = edit(json!({ "trim": { "start_ms": 1000, "end_ms": 3000 }, "bin_original": true }))
            .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        let cut = r.json();
        assert_eq!(cut["mime"], "video/webm");
        assert_eq!(cut["tags"], json!(["clip"]));
        let ms = cut["duration_ms"].as_i64().unwrap();
        assert!((1_900..=2_500).contains(&ms), "{ms}");
        assert!(cut["thumb_url"].is_string());
        let original = app.get(&format!("/api/v1/posts/{id}"), &t).await.json();
        assert_eq!(original["deleted"], true);
        app.finish().await;
    }
}
