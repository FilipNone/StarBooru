//! Chunked uploads, for files too big for one request: `POST /uploads`, a
//! `PUT` per chunk in any order, then `POST /uploads/{id}/finish`.

use std::io::SeekFrom;
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use starbooru_core::tag::normalize_tags;
use starbooru_core::{Post, UPLOAD_CHUNK_SIZE, UploadSession, UploadStart, upload_chunk_range};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use super::posts::{self, Stored};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage;
use crate::uploads::{Phase, Upload};

fn gone() -> AppError {
    AppError::Conflict {
        code: "upload_gone",
        message: "this upload stopped, as it sat idle or the server restarted: send the file again"
            .into(),
    }
}

fn find(state: &AppState, id: &str, user_id: i64) -> AppResult<Arc<Upload>> {
    state.uploads.get(id, user_id).ok_or_else(gone)
}

/// `POST /uploads`.
pub async fn start(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<UploadStart>,
) -> AppResult<Json<UploadSession>> {
    if req.size == 0 {
        return Err(AppError::BadRequest("the file is empty".into()));
    }
    let limit = u64::try_from(state.config.max_upload_bytes).unwrap_or(u64::MAX);
    if req.size > limit {
        return Err(AppError::PayloadTooLarge);
    }
    let file_size = i64::try_from(req.size).map_err(|_| AppError::PayloadTooLarge)?;
    let tags = normalize_tags(req.tags.iter().map(String::as_str))?;
    let source = req.source.map(posts::clean_source).transpose()?.flatten();
    posts::check_quota(&state.db, user.id, file_size).await?;

    let mut nonce = [0u8; 16];
    rand::fill(&mut nonce);
    let id = hex::encode(nonce);
    let path = state.storage.scratch_path("upload");
    let file = tokio::fs::File::create(&path).await?;
    if let Err(e) = file.set_len(req.size).await {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(e.into());
    }
    let chunks = starbooru_core::upload_chunks(req.size, UPLOAD_CHUNK_SIZE);
    let upload = Upload::new(
        id.clone(),
        user.id,
        path,
        req.size,
        UPLOAD_CHUNK_SIZE,
        chunks,
        tags,
        req.rating,
        source,
    );
    if let Some(dropped) = state.uploads.add(Arc::new(upload)) {
        dropped.discard().await;
    }
    Ok(Json(UploadSession {
        id,
        chunk_size: UPLOAD_CHUNK_SIZE,
        chunks,
    }))
}

/// `PUT /uploads/{id}/chunks/{index}`. Sending a chunk again is harmless. The
/// body arrives whole before anything is locked, so a slow client holds up
/// nothing but itself.
pub async fn put_chunk(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, index)): Path<(String, u32)>,
    body: Bytes,
) -> AppResult<StatusCode> {
    let upload = find(&state, &id, user.id)?;
    let (offset, len) = upload_chunk_range(upload.size, upload.chunk_size, index)
        .ok_or_else(|| AppError::BadRequest(format!("this upload has no chunk {index}")))?;
    if u64::try_from(body.len()).ok() != Some(len) {
        return Err(AppError::BadRequest(format!(
            "chunk {index} should be {len} bytes, not {}",
            body.len()
        )));
    }
    // The first chunk tells what the file is, so a file that will be refused
    // doesn't have to arrive whole first.
    if index == 0
        && let Err(e) = storage::sniff(&body)
    {
        state.uploads.remove(&id);
        upload.discard().await;
        return Err(e);
    }

    let phase = upload.phase.read().await;
    match *phase {
        Phase::Receiving => {}
        Phase::Gone => return Err(gone()),
        Phase::Added(_) | Phase::Exists(_) => return Ok(StatusCode::NO_CONTENT),
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&upload.path)
        .await?;
    file.seek(SeekFrom::Start(offset)).await?;
    file.write_all(&body).await?;
    file.flush().await?;
    upload.mark_received(index);
    Ok(StatusCode::NO_CONTENT)
}

/// Moves a complete upload into storage and makes its post.
async fn store(state: &AppState, upload: &Arc<Upload>) -> AppResult<Stored> {
    let file_size = i64::try_from(upload.size).map_err(AppError::internal)?;
    let (worker, path) = (state.clone(), upload.path.clone());
    let info = tokio::task::spawn_blocking(move || worker.storage.ingest_file(&path)).await??;
    posts::record_post(
        state,
        upload.user_id,
        &info,
        file_size,
        upload.rating,
        upload.source.clone(),
        &upload.tags,
    )
    .await
}

/// `POST /uploads/{id}/finish`. A client whose answer was lost may ask again: a
/// finish under way is waited for, and a finished upload answers the same.
pub async fn finish(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Post>> {
    let upload = find(&state, &id, user.id)?;
    let mut phase = upload.phase.write().await;
    if *phase == Phase::Receiving {
        let missing = upload.missing();
        if missing > 0 {
            return Err(AppError::Conflict {
                code: "chunks_missing",
                message: format!("{missing} chunks of this upload haven't arrived"),
            });
        }
        let stored = store(&state, &upload).await;
        upload.remove_file().await;
        *phase = match &stored {
            Ok(Stored::Added(post)) => Phase::Added(*post),
            Ok(Stored::Exists(post)) => Phase::Exists(*post),
            Err(_) => Phase::Gone,
        };
        if let Err(e) = stored {
            state.uploads.remove(&id);
            return Err(e);
        }
    }
    let outcome = *phase;
    drop(phase);
    match outcome {
        Phase::Added(post) => posts::load(&state, user.id, post).await.map(Json),
        Phase::Exists(post) => Err(posts::duplicate_of(&state, post).await?),
        Phase::Receiving | Phase::Gone => Err(gone()),
    }
}

/// `DELETE /uploads/{id}`: gives up on an upload. What already became a post
/// stays, and an upload that is gone is no error.
pub async fn cancel(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    if let Some(upload) = state.uploads.get(&id, user.id) {
        state.uploads.remove(&id);
        upload.discard().await;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Method, Request, header};
    use serde_json::json;
    use sha2::{Digest, Sha256};

    use crate::testing::{self, TestApp};

    use super::*;

    const API: &str = "/api/v1/uploads";

    /// A file the server takes for a video, of `size` bytes that differ by
    /// `seed`. Without ffmpeg in the tests nothing reads past its start.
    fn video(size: usize, seed: u8) -> Vec<u8> {
        let mut bytes = b"\0\0\0\x18ftypisom\0\0\x02\0isomiso2".to_vec();
        let mut value = seed;
        bytes.resize_with(size, || {
            value = value.wrapping_mul(31).wrapping_add(7);
            value
        });
        bytes
    }

    async fn begin(
        app: &TestApp,
        token: &str,
        size: usize,
        extra: serde_json::Value,
    ) -> (u16, serde_json::Value) {
        let mut body = json!({"size": size});
        if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            body.extend(extra.clone());
        }
        let r = app.call(Method::POST, API, Some(token), Some(body)).await;
        (r.status.as_u16(), r.json())
    }

    async fn put(app: &TestApp, token: &str, id: &str, index: u32, bytes: &[u8]) -> testing::Resp {
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("{API}/{id}/chunks/{index}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(bytes.to_vec()))
            .unwrap();
        app.send(req).await
    }

    async fn end(app: &TestApp, token: &str, id: &str) -> testing::Resp {
        app.call(
            Method::POST,
            &format!("{API}/{id}/finish"),
            Some(token),
            None,
        )
        .await
    }

    /// Sends every chunk of `file`, last first.
    async fn send_all(app: &TestApp, token: &str, id: &str, file: &[u8]) {
        let chunks: Vec<&[u8]> = file.chunks(UPLOAD_CHUNK_SIZE as usize).collect();
        for (index, chunk) in chunks.iter().enumerate().rev() {
            let r = put(app, token, id, index as u32, chunk).await;
            assert_eq!(r.status, StatusCode::NO_CONTENT, "chunk {index}");
        }
    }

    fn scratch_files(app: &TestApp) -> usize {
        std::fs::read_dir(app.state.config.data_dir.join("tmp"))
            .unwrap()
            .count()
    }

    #[tokio::test]
    async fn chunks_in_any_order_make_the_file() {
        let Some(app) = testing::app().await else {
            return;
        };
        let token = app.register("alice").await;
        let file = video(5 * (1 << 20) + 123, 1);
        let (status, session) = begin(
            &app,
            &token,
            file.len(),
            json!({"tags": ["Big_Video", "cat"], "rating": "questionable", "source": "https://a.example/1"}),
        )
        .await;
        assert_eq!(status, 200, "{session}");
        assert_eq!(session["chunks"], 3);
        assert_eq!(session["chunk_size"], UPLOAD_CHUNK_SIZE);
        let id = session["id"].as_str().unwrap();

        // Nothing is on the way to becoming a post until every chunk is in.
        let early = end(&app, &token, id).await;
        assert_eq!(early.status, StatusCode::CONFLICT);
        assert_eq!(early.json()["code"], "chunks_missing");

        // A chunk sent twice changes nothing.
        let first = file.get(..UPLOAD_CHUNK_SIZE as usize).unwrap();
        assert_eq!(
            put(&app, &token, id, 0, first).await.status,
            StatusCode::NO_CONTENT
        );
        send_all(&app, &token, id, &file).await;

        let done = end(&app, &token, id).await;
        assert_eq!(done.status, StatusCode::OK, "{:?}", done.json());
        let post = done.json();
        assert_eq!(post["kind"], "video");
        assert_eq!(post["file_size"], file.len());
        assert_eq!(post["rating"], "questionable");
        assert_eq!(post["source"], "https://a.example/1");
        let sha = hex::encode(Sha256::digest(&file));
        assert_eq!(post["sha256"], sha);
        let tags = post["tags"].as_array().unwrap();
        assert!(tags.iter().any(|t| t == "big_video"), "{tags:?}");

        let stored = app.state.storage.original_path(&sha, "mp4").unwrap();
        assert_eq!(std::fs::read(stored).unwrap(), file);
        assert_eq!(scratch_files(&app), 0);

        // The answer was lost on the way: asking again gives the same post.
        let again = end(&app, &token, id).await;
        assert_eq!(again.status, StatusCode::OK);
        assert_eq!(again.json()["id"], post["id"]);
        app.finish().await;
    }

    #[tokio::test]
    async fn an_image_is_decoded_like_any_upload() {
        let Some(app) = testing::app().await else {
            return;
        };
        let token = app.register("alice").await;
        let png = testing::noise_png(1200, 1000, 3);
        assert!(png.len() as u64 > UPLOAD_CHUNK_SIZE, "{}", png.len());
        let (status, session) = begin(&app, &token, png.len(), json!({})).await;
        assert_eq!(status, 200);
        let id = session["id"].as_str().unwrap();
        send_all(&app, &token, id, &png).await;
        let post = end(&app, &token, id).await;
        assert_eq!(post.status, StatusCode::OK, "{:?}", post.json());
        assert_eq!(post.json()["width"], 1200);
        assert!(post.json()["thumb_url"].is_string());
        assert_eq!(scratch_files(&app), 0);
        app.finish().await;
    }

    #[tokio::test]
    async fn a_file_already_here_is_a_duplicate_every_time_it_is_asked() {
        let Some(app) = testing::app().await else {
            return;
        };
        let token = app.register("alice").await;
        let file = video(3 << 20, 2);
        let first = app.upload(&token, &file, &[]).await;
        assert_eq!(first.status, StatusCode::OK);

        let (_, session) = begin(&app, &token, file.len(), json!({})).await;
        let id = session["id"].as_str().unwrap();
        send_all(&app, &token, id, &file).await;
        for _ in 0..2 {
            let r = end(&app, &token, id).await;
            assert_eq!(r.status, StatusCode::CONFLICT);
            assert_eq!(r.json()["code"], "duplicate");
        }
        assert_eq!(scratch_files(&app), 0);
        app.finish().await;
    }

    #[tokio::test]
    async fn refuses_a_bad_upload_before_it_is_sent() {
        let Some(app) = testing::app_with(|c| c.max_upload_bytes = 4 << 20).await else {
            return;
        };
        let token = app.register("alice").await;
        assert_eq!(begin(&app, &token, 0, json!({})).await.0, 400);
        assert_eq!(begin(&app, &token, (4 << 20) + 1, json!({})).await.0, 413);
        let (status, body) = begin(&app, &token, 1 << 20, json!({"tags": ["-x"]})).await;
        assert_eq!(status, 400, "{body}");
        let anonymous = app
            .call(Method::POST, API, None, Some(json!({"size": 1})))
            .await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
        assert_eq!(scratch_files(&app), 0);

        // The storage limit counts before a byte is sent.
        sqlx::query("UPDATE users SET quota_bytes = 100")
            .execute(app.db())
            .await
            .unwrap();
        let (status, body) = begin(&app, &token, 1 << 20, json!({})).await;
        assert_eq!(status, 403);
        assert_eq!(body["code"], "quota_exceeded");
        app.finish().await;
    }

    #[tokio::test]
    async fn checks_every_chunk() {
        let Some(app) = testing::app().await else {
            return;
        };
        let token = app.register("alice").await;
        let file = video((2 << 20) + 10, 3);
        let (_, session) = begin(&app, &token, file.len(), json!({})).await;
        let id = session["id"].as_str().unwrap();
        // Past the end of the file, even at an offset that overflows.
        assert_eq!(
            put(&app, &token, id, 2, &[0; 10]).await.status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(&app, &token, id, u32::MAX, &[0; 10]).await.status,
            StatusCode::BAD_REQUEST
        );
        // The wrong length, for a whole chunk and the last.
        assert_eq!(
            put(&app, &token, id, 0, &[0; 10]).await.status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(&app, &token, id, 1, &[0; 11]).await.status,
            StatusCode::BAD_REQUEST
        );
        // Bigger than any chunk can be.
        let huge = vec![0; (UPLOAD_CHUNK_SIZE + 1) as usize];
        assert_eq!(
            put(&app, &token, id, 0, &huge).await.status,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        app.finish().await;
    }

    #[tokio::test]
    async fn the_first_chunk_refuses_a_file_that_is_no_picture() {
        let Some(app) = testing::app().await else {
            return;
        };
        let token = app.register("alice").await;
        let (_, session) = begin(&app, &token, 3 << 20, json!({})).await;
        let id = session["id"].as_str().unwrap();
        assert_eq!(scratch_files(&app), 1);
        let chunk = vec![b'%'; UPLOAD_CHUNK_SIZE as usize];
        assert_eq!(
            put(&app, &token, id, 0, &chunk).await.status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        assert_eq!(scratch_files(&app), 0);
        let after = put(&app, &token, id, 1, &vec![0; 1 << 20]).await;
        assert_eq!(after.status, StatusCode::CONFLICT);
        assert_eq!(after.json()["code"], "upload_gone");
        app.finish().await;
    }

    #[tokio::test]
    async fn an_upload_is_its_owners_alone() {
        let Some(app) = testing::app().await else {
            return;
        };
        let alice = app.register("alice").await;
        let bob = app.register("bob").await;
        let file = video(3 << 20, 4);
        let (_, session) = begin(&app, &alice, file.len(), json!({})).await;
        let id = session["id"].as_str().unwrap();
        let first = file.get(..UPLOAD_CHUNK_SIZE as usize).unwrap();
        assert_eq!(
            put(&app, &bob, id, 0, first).await.status,
            StatusCode::CONFLICT
        );
        assert_eq!(end(&app, &bob, id).await.status, StatusCode::CONFLICT);
        let cancelled = app
            .call(Method::DELETE, &format!("{API}/{id}"), Some(&bob), None)
            .await;
        assert_eq!(cancelled.status, StatusCode::NO_CONTENT);
        // Bob's cancel did nothing to it.
        send_all(&app, &alice, id, &file).await;
        assert_eq!(end(&app, &alice, id).await.status, StatusCode::OK);
        app.finish().await;
    }

    #[tokio::test]
    async fn cancelling_removes_the_file() {
        let Some(app) = testing::app().await else {
            return;
        };
        let token = app.register("alice").await;
        let (_, session) = begin(&app, &token, 3 << 20, json!({})).await;
        let id = session["id"].as_str().unwrap();
        assert_eq!(scratch_files(&app), 1);
        let path = format!("{API}/{id}");
        for _ in 0..2 {
            let r = app.call(Method::DELETE, &path, Some(&token), None).await;
            assert_eq!(r.status, StatusCode::NO_CONTENT);
        }
        assert_eq!(scratch_files(&app), 0);
        let after = end(&app, &token, id).await;
        assert_eq!(after.json()["code"], "upload_gone");
        app.finish().await;
    }

    #[tokio::test]
    async fn opening_too_many_uploads_removes_the_oldest_file() {
        let Some(app) = testing::app().await else {
            return;
        };
        let token = app.register("alice").await;
        let mut ids = Vec::new();
        for _ in 0..8 {
            let (_, session) = begin(&app, &token, 1 << 20, json!({})).await;
            ids.push(session["id"].as_str().unwrap().to_owned());
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert_eq!(scratch_files(&app), 8);
        let (status, _) = begin(&app, &token, 1 << 20, json!({})).await;
        assert_eq!(status, 200);
        assert_eq!(scratch_files(&app), 8);
        assert_eq!(
            end(&app, &token, &ids[0]).await.json()["code"],
            "upload_gone"
        );
        app.finish().await;
    }
}
