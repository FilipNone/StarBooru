//! The recycle bin: emptying it by hand and the automatic purge.

use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use starbooru_core::{PurgeResult, Role};
use tracing::{info, warn};

use super::posts::{Purge, purge_posts};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::settings;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct EmptyParams {
    /// Only purge posts deleted at least this many days ago.
    #[serde(default)]
    older_than_days: u32,
}

/// `POST /trash/empty?older_than_days=`: admins only.
pub async fn empty(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<EmptyParams>,
) -> AppResult<Json<PurgeResult>> {
    if user.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    let purged = purge_older_than(&state, params.older_than_days).await?;
    Ok(Json(PurgeResult { purged }))
}

async fn purge_older_than(state: &AppState, days: u32) -> AppResult<u64> {
    purge_posts(state, Purge::OlderThanDays(days)).await
}

/// Background task: hourly, purges posts older than the recycle bin's
/// limit (`STARBOORU_TRASH_DAYS`, or as changed in the admin settings).
pub async fn auto_purge(state: AppState) {
    let mut tick = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tick.tick().await;
        let days = match settings::current(&state).await {
            Ok(s) if s.trash_days > 0 => s.trash_days,
            Ok(_) => continue,
            Err(e) => {
                warn!("recycle bin purge skipped: {e}");
                continue;
            }
        };
        match purge_older_than(&state, days).await {
            Ok(0) => {}
            Ok(n) => info!("recycle bin: purged {n} posts older than {days} days"),
            Err(e) => warn!("recycle bin purge failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};

    use crate::testing;

    #[tokio::test]
    async fn purge_removes_post_and_files() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let user = app.register("someone").await;
        let id = app.upload_png(&user, 1, "sky").await;
        let post = app.get(&format!("/api/v1/posts/{id}"), &user).await.json();
        let thumb_url = post["thumb_url"].as_str().unwrap().to_owned();

        // A live post can't be purged.
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/purge"),
                Some(&user),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT);

        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/posts/{id}"),
                Some(&user),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        // Another user can't purge it.
        let other = app.register("other").await;
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/purge"),
                Some(&other),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);

        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/purge"),
                Some(&user),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        let r = app.get(&format!("/api/v1/posts/{id}"), &admin).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app.call(Method::GET, &thumb_url, None, None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "thumbnail file is gone");

        // The same file can be uploaded again afterwards.
        app.upload_png(&user, 1, "sky").await;
        app.finish().await;
    }

    #[tokio::test]
    async fn empty_bin_is_admin_only_and_respects_age() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let user = app.register("someone").await;
        let old = app.upload_png(&user, 2, "a").await;
        let fresh = app.upload_png(&user, 3, "a").await;
        let live = app.upload_png(&user, 4, "a").await;
        for id in [old, fresh] {
            app.call(
                Method::DELETE,
                &format!("/api/v1/posts/{id}"),
                Some(&user),
                None,
            )
            .await;
        }
        sqlx::query("UPDATE posts SET deleted_at = now() - interval '40 days' WHERE id = $1")
            .bind(old)
            .execute(app.db())
            .await
            .unwrap();

        let r = app
            .call(Method::POST, "/api/v1/trash/empty", Some(&user), None)
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);

        let r = app
            .call(
                Method::POST,
                "/api/v1/trash/empty?older_than_days=30",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.json()["purged"], 1);
        let r = app
            .call(Method::POST, "/api/v1/trash/empty", Some(&admin), None)
            .await;
        assert_eq!(r.json()["purged"], 1);
        let left: Vec<i64> = sqlx::query_scalar("SELECT id FROM posts")
            .fetch_all(app.db())
            .await
            .unwrap();
        assert_eq!(left, vec![live]);
        // Tag counts stay right: only the live post carries `a`.
        let count: i32 = sqlx::query_scalar("SELECT post_count FROM tags WHERE name = 'a'")
            .fetch_one(app.db())
            .await
            .unwrap();
        assert_eq!(count, 1);
        app.finish().await;
    }
}
