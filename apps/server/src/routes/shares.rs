//! Share links. Their owners make and remove them; anyone with a link's
//! token sees its posts through `GET /shared/{token}`, without an account.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use starbooru_core::share::{MAX_SHARES, SHARE_TOKEN_CHARS, is_share_token};
use starbooru_core::{MediaKind, NewShare, Share, SharedPost, SharedView};

use super::posts::{media_urls, sample_url};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::settings;
use crate::state::AppState;

#[derive(FromRow)]
struct ShareRow {
    id: i64,
    token: String,
    title: Option<String>,
    posts: i32,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    views: i64,
    last_viewed_at: Option<DateTime<Utc>>,
    cover_sha: Option<Vec<u8>>,
    cover_ext: Option<String>,
    cover_thumb: Option<bool>,
}

/// The caller's links (or the one with id `$2`, when not NULL) with their
/// cover: the first of their posts not in the bin.
const SELECT_SHARES: &str = "SELECT s.id, s.token, s.title, cardinality(s.post_ids) AS posts, \
     s.created_at, s.expires_at, s.views, s.last_viewed_at, \
     c.sha256 AS cover_sha, c.ext AS cover_ext, c.has_thumb AS cover_thumb \
     FROM shares s LEFT JOIN LATERAL ( \
         SELECT p.sha256, p.ext, p.has_thumb \
         FROM unnest(s.post_ids) WITH ORDINALITY AS u(id, n) \
         JOIN posts p ON p.id = u.id \
         WHERE p.deleted_at IS NULL ORDER BY u.n LIMIT 1) c ON true \
     WHERE s.user_id = $1 AND ($2::bigint IS NULL OR s.id = $2) \
     ORDER BY s.created_at DESC, s.id DESC";

fn to_share(state: &AppState, row: ShareRow) -> Share {
    let cover_url = match (row.cover_sha, row.cover_ext, row.cover_thumb) {
        (Some(sha), Some(ext), Some(true)) => media_urls(state, &hex::encode(sha), &ext, true).1,
        _ => None,
    };
    Share {
        id: row.id,
        token: row.token,
        title: row.title,
        posts: u32::try_from(row.posts).unwrap_or(0),
        cover_url,
        created_at: row.created_at,
        expires_at: row.expires_at,
        views: row.views,
        last_viewed_at: row.last_viewed_at,
    }
}

/// `GET /shares`: the caller's links, newest first, expired ones included.
pub async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Share>>> {
    let rows: Vec<ShareRow> = sqlx::query_as(SELECT_SHARES)
        .bind(user.id)
        .bind(None::<i64>)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(
        rows.into_iter().map(|r| to_share(&state, r)).collect(),
    ))
}

/// `POST /shares`: every post must exist and be out of the bin.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<NewShare>,
) -> AppResult<Json<Share>> {
    let req = req
        .clean()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let mut tx = state.db.begin().await?;
    // Serialises one user's links, so the limit holds.
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM shares WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= i64::try_from(MAX_SHARES).unwrap_or(i64::MAX) {
        return Err(AppError::BadRequest(format!(
            "you can keep at most {MAX_SHARES} links; remove some first"
        )));
    }
    let live: i64 =
        sqlx::query_scalar("SELECT count(*) FROM posts WHERE id = ANY($1) AND deleted_at IS NULL")
            .bind(&req.post_ids)
            .fetch_one(&mut *tx)
            .await?;
    if usize::try_from(live).ok() != Some(req.post_ids.len()) {
        return Err(AppError::BadRequest(
            "some of these posts don't exist or are in the recycle bin".into(),
        ));
    }
    let token = new_token();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO shares (user_id, token, title, post_ids, expires_at) \
         VALUES ($1, $2, $3, $4, now() + make_interval(days => $5)) RETURNING id",
    )
    .bind(user.id)
    .bind(&token)
    .bind(&req.title)
    .bind(&req.post_ids)
    .bind(req.days.and_then(|d| i32::try_from(d).ok()))
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let row: ShareRow = sqlx::query_as(SELECT_SHARES)
        .bind(user.id)
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(to_share(&state, row)))
}

/// 128 random bits as lowercase hex.
fn new_token() -> String {
    let mut bytes = [0u8; SHARE_TOKEN_CHARS / 2];
    rand::fill(&mut bytes);
    hex::encode(bytes)
}

/// `DELETE /shares/{id}`: the link stops working at once.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let done = sqlx::query("DELETE FROM shares WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(FromRow)]
struct SharedRow {
    kind: i16,
    mime: String,
    width: Option<i32>,
    height: Option<i32>,
    duration_ms: Option<i64>,
    sha256: Vec<u8>,
    ext: String,
    has_thumb: bool,
    has_sample: Option<bool>,
}

/// Title, posts and expiry of a link just opened.
type OpenedRow = (Option<String>, Vec<i64>, Option<DateTime<Utc>>);

/// `GET /shared/{token}`: no account needed. Unknown, expired and removed
/// links, and links of disabled accounts, are all 404.
pub async fn view(State(state): State<AppState>, Path(token): Path<String>) -> AppResult<Response> {
    if !is_share_token(&token) {
        return Err(AppError::NotFound);
    }
    let found: Option<OpenedRow> = sqlx::query_as(
        "UPDATE shares s SET views = s.views + 1, last_viewed_at = now() \
         FROM users u \
         WHERE s.token = $1 AND u.id = s.user_id AND u.disabled_at IS NULL \
           AND (s.expires_at IS NULL OR s.expires_at > now()) \
         RETURNING s.title, s.post_ids, s.expires_at",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?;
    let (title, post_ids, expires_at) = found.ok_or(AppError::NotFound)?;
    let rows: Vec<SharedRow> = sqlx::query_as(
        "SELECT p.kind, p.mime, p.width, p.height, p.duration_ms, p.sha256, p.ext, p.has_thumb, \
         p.has_sample FROM unnest($1::bigint[]) WITH ORDINALITY AS u(id, n) \
         JOIN posts p ON p.id = u.id \
         WHERE p.deleted_at IS NULL ORDER BY u.n",
    )
    .bind(&post_ids)
    .fetch_all(&state.db)
    .await?;
    let posts = rows
        .into_iter()
        .map(|r| {
            let sha = hex::encode(&r.sha256);
            let (file_url, thumb_url) = media_urls(&state, &sha, &r.ext, r.has_thumb);
            SharedPost {
                kind: MediaKind::from_db(r.kind).unwrap_or(MediaKind::Image),
                mime: r.mime,
                width: r.width,
                height: r.height,
                duration_ms: r.duration_ms,
                file_url,
                thumb_url,
                sample_url: sample_url(&state, &sha, r.has_sample),
            }
        })
        .collect();
    let view = SharedView {
        title,
        server: settings::current(&state).await?.name,
        expires_at,
        posts,
    };
    let mut resp = Json(view).into_response();
    let headers = resp.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use axum::http::Method;
    use serde_json::json;

    use crate::testing;

    #[tokio::test]
    async fn links_show_their_posts_until_removed() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let bob = app.register("bob").await;
        let a = app.upload_png(&admin, 1, "secret_tag").await;
        let b = app.upload_png(&admin, 2, "").await;
        let c = app.upload_png(&admin, 3, "").await;
        let make = |token: String, body: serde_json::Value| {
            let app = &app;
            async move {
                app.call(Method::POST, "/api/v1/shares", Some(&token), Some(body))
                    .await
            }
        };

        // Checked before anything is stored.
        for bad in [
            json!({ "post_ids": [] }),
            json!({ "post_ids": [a, 999_999] }),
            json!({ "post_ids": [a], "days": 0 }),
            json!({ "post_ids": [a], "title": "x".repeat(101) }),
        ] {
            assert_eq!(make(bob.clone(), bad.clone()).await.status, 400, "{bad}");
        }
        let r = app
            .call(
                Method::POST,
                "/api/v1/shares",
                None,
                Some(json!({ "post_ids": [a] })),
            )
            .await;
        assert_eq!(r.status, 401);

        // Anyone with an account can share any post; order is kept, repeats dropped.
        let r = make(
            bob.clone(),
            json!({ "post_ids": [c, a, c, b], "title": " Trip " }),
        )
        .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        let share = r.json();
        assert_eq!(share["title"], "Trip");
        assert_eq!(share["posts"], 3);
        assert_eq!(share["views"], 0);
        assert!(share["expires_at"].is_null());
        assert!(
            share["cover_url"]
                .as_str()
                .unwrap()
                .starts_with("/media/thumb/")
        );
        let token = share["token"].as_str().unwrap().to_owned();
        assert_eq!(token.len(), 32);

        // No account needed; media only, no tags or ids.
        let path = format!("/api/v1/shared/{token}");
        let r = app.call(Method::GET, &path, None, None).await;
        assert_eq!(r.status, 200);
        assert_eq!(r.headers["cache-control"], "no-store");
        let view = r.json();
        assert_eq!(view["title"], "Trip");
        assert_eq!(view["server"], "StarBooru");
        let posts = view["posts"].as_array().unwrap();
        assert_eq!(posts.len(), 3);
        assert!(!r.body.windows(10).any(|w| w == b"secret_tag"));
        assert!(posts[0].get("id").is_none());
        let file = posts[0]["file_url"].as_str().unwrap().to_owned();
        assert_eq!(app.call(Method::GET, &file, None, None).await.status, 200);
        let r = app.get("/api/v1/shares", &bob).await;
        assert_eq!(r.json()[0]["views"], 1);
        assert!(r.json()[0]["last_viewed_at"].is_string());
        // Only the owner lists the link.
        assert_eq!(app.get("/api/v1/shares", &admin).await.json(), json!([]));

        // Binned posts drop out, and come back when restored.
        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/posts/{c}"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, 204);
        let r = app.call(Method::GET, &path, None, None).await;
        assert_eq!(r.json()["posts"].as_array().unwrap().len(), 2);
        assert_eq!(
            make(bob.clone(), json!({ "post_ids": [c] })).await.status,
            400
        );

        // Bad, unknown and expired tokens look alike.
        for missing in ["nope", "../../x", "0123456789abcdef0123456789abcdef"] {
            let r = app
                .call(
                    Method::GET,
                    &format!("/api/v1/shared/{missing}"),
                    None,
                    None,
                )
                .await;
            assert_eq!(r.status, 404, "{missing}");
        }
        let r = make(bob.clone(), json!({ "post_ids": [a], "days": 7 })).await;
        let week = r.json();
        assert!(week["expires_at"].is_string());
        let week_path = format!("/api/v1/shared/{}", week["token"].as_str().unwrap());
        assert_eq!(
            app.call(Method::GET, &week_path, None, None).await.status,
            200
        );
        sqlx::query("UPDATE shares SET expires_at = now() - interval '1 minute' WHERE id = $1")
            .bind(week["id"].as_i64().unwrap())
            .execute(app.db())
            .await
            .unwrap();
        assert_eq!(
            app.call(Method::GET, &week_path, None, None).await.status,
            404
        );

        // A disabled account's links stop working.
        let bob_id = app.get("/api/v1/auth/me", &bob).await.json()["id"].clone();
        let user = format!("/api/v1/admin/users/{bob_id}");
        let patch = |disabled: bool| {
            let app = &app;
            let (admin, user) = (admin.clone(), user.clone());
            async move {
                app.call(
                    Method::PATCH,
                    &user,
                    Some(&admin),
                    Some(json!({ "disabled": disabled })),
                )
                .await
                .status
            }
        };
        assert_eq!(patch(true).await, 200);
        assert_eq!(app.call(Method::GET, &path, None, None).await.status, 404);
        assert_eq!(patch(false).await, 200);
        assert_eq!(app.call(Method::GET, &path, None, None).await.status, 200);
        // Disabling ended bob's sessions.
        let r = app
            .call(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "username": "bob", "password": testing::PASSWORD })),
            )
            .await;
        let bob = r.json()["token"].as_str().unwrap().to_owned();

        // Removing: owner only, at once.
        let remove = format!("/api/v1/shares/{}", share["id"]);
        let r = app.call(Method::DELETE, &remove, Some(&admin), None).await;
        assert_eq!(r.status, 404);
        let r = app.call(Method::DELETE, &remove, Some(&bob), None).await;
        assert_eq!(r.status, 204);
        assert_eq!(app.call(Method::GET, &path, None, None).await.status, 404);
        app.finish().await;
    }
}
