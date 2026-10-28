//! Administration: statistics, server settings and accounts. Admins only;
//! changes need a sign-in session, so an API key can read but not change.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::FromRow;
use starbooru_core::admin::STATS_DAYS;
use starbooru_core::{
    AdminUser, DayCount, MediaKind, PasswordReset, Role, ServerSettings, ServerStats, UserUpdate,
    clean_settings,
};

use crate::auth::{self, AuthUser, hash_password};
use crate::error::{AppError, AppResult};
use crate::settings;
use crate::state::AppState;

/// Serialises account changes, so two admins can't demote each other at once
/// and leave none.
const ACCOUNTS_LOCK: i64 = 7276;

fn count(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

/// An admin, signed in with a session.
pub(super) fn require_admin_session(user: &AuthUser) -> AppResult<()> {
    user.require_admin()?;
    user.session_id().map(|_| ())
}

#[derive(FromRow)]
struct Totals {
    users: i64,
    disabled_users: i64,
    images: i64,
    animations: i64,
    videos: i64,
    bin_posts: i64,
    media_bytes: i64,
    bin_bytes: i64,
    tags: i64,
    database_bytes: i64,
}

/// `GET /admin/stats`.
pub async fn stats(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<ServerStats>> {
    user.require_admin()?;
    let t: Totals = sqlx::query_as(
        "SELECT (SELECT count(*) FROM users) AS users, \
                (SELECT count(*) FROM users WHERE disabled_at IS NOT NULL) AS disabled_users, \
                count(*) FILTER (WHERE deleted_at IS NULL AND kind = $1) AS images, \
                count(*) FILTER (WHERE deleted_at IS NULL AND kind = $2) AS animations, \
                count(*) FILTER (WHERE deleted_at IS NULL AND kind = $3) AS videos, \
                count(*) FILTER (WHERE deleted_at IS NOT NULL) AS bin_posts, \
                coalesce(sum(file_size), 0)::bigint AS media_bytes, \
                coalesce(sum(file_size) FILTER (WHERE deleted_at IS NOT NULL), 0)::bigint \
                    AS bin_bytes, \
                (SELECT count(*) FROM tags) AS tags, \
                pg_database_size(current_database()) AS database_bytes \
         FROM posts",
    )
    .bind(MediaKind::Image.to_db())
    .bind(MediaKind::Animation.to_db())
    .bind(MediaKind::Video.to_db())
    .fetch_one(&state.db)
    .await?;
    let days: Vec<(NaiveDate, i64)> = sqlx::query_as(
        "WITH days AS ( \
             SELECT (now() AT TIME ZONE 'UTC')::date - n AS day \
             FROM generate_series(0, $1 - 1) AS n \
         ), recent AS ( \
             SELECT (created_at AT TIME ZONE 'UTC')::date AS day FROM posts \
             WHERE created_at >= ((now() AT TIME ZONE 'UTC')::date - ($1 - 1)) AT TIME ZONE 'UTC' \
         ) \
         SELECT d.day, count(r.day) FROM days d LEFT JOIN recent r ON r.day = d.day \
         GROUP BY d.day ORDER BY d.day",
    )
    .bind(i32::try_from(STATS_DAYS).unwrap_or(30))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ServerStats {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        started_at: state.started_at,
        users: count(t.users),
        disabled_users: count(t.disabled_users),
        images: count(t.images),
        animations: count(t.animations),
        videos: count(t.videos),
        bin_posts: count(t.bin_posts),
        tags: count(t.tags),
        media_bytes: count(t.media_bytes),
        bin_bytes: count(t.bin_bytes),
        database_bytes: count(t.database_bytes),
        uploads_by_day: days
            .into_iter()
            .map(|(day, posts)| DayCount {
                day,
                posts: count(posts),
            })
            .collect(),
        max_upload_bytes: state.config.max_upload_bytes as u64,
        video_thumbnails: state.storage.has_tools(),
        url_import: state.fetcher.is_some(),
    }))
}

/// `GET /admin/settings`.
pub async fn get_settings(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<ServerSettings>> {
    user.require_admin()?;
    settings::current(&state).await.map(Json)
}

/// `PUT /admin/settings`: takes effect at once.
pub async fn put_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ServerSettings>,
) -> AppResult<Json<ServerSettings>> {
    require_admin_session(&user)?;
    let clean = clean_settings(&req).map_err(|e| AppError::BadRequest(e.to_string()))?;
    settings::save(&state, &clean).await?;
    settings::current(&state).await.map(Json)
}

#[derive(FromRow)]
struct UserRow {
    id: i64,
    username: String,
    role: String,
    created_at: DateTime<Utc>,
    disabled: bool,
    posts: i64,
    bytes: i64,
    quota_bytes: Option<i64>,
    last_active: Option<DateTime<Utc>>,
}

impl TryFrom<UserRow> for AdminUser {
    type Error = AppError;

    fn try_from(r: UserRow) -> AppResult<Self> {
        Ok(AdminUser {
            id: r.id,
            username: r.username,
            role: Role::from_db(&r.role).ok_or_else(|| AppError::internal("bad role in db"))?,
            created_at: r.created_at,
            disabled: r.disabled,
            posts: count(r.posts),
            bytes: count(r.bytes),
            quota_bytes: r.quota_bytes.and_then(|q| u64::try_from(q).ok()),
            last_active: r.last_active,
        })
    }
}

/// Every account with `$1` NULL, else just that one.
const USERS_SQL: &str = "SELECT u.id, u.username, u.role, u.created_at, \
        u.disabled_at IS NOT NULL AS disabled, \
        coalesce(p.posts, 0) AS posts, coalesce(p.bytes, 0)::bigint AS bytes, u.quota_bytes, \
        greatest((SELECT max(last_used_at) FROM sessions s WHERE s.user_id = u.id), \
                 (SELECT max(last_used_at) FROM api_keys k WHERE k.user_id = u.id)) \
            AS last_active \
    FROM users u \
    LEFT JOIN (SELECT uploader_id, count(*) AS posts, sum(file_size) AS bytes \
               FROM posts GROUP BY uploader_id) p ON p.uploader_id = u.id \
    WHERE $1::bigint IS NULL OR u.id = $1 \
    ORDER BY u.id";

/// `GET /admin/users`: every account, oldest first, with what it uploaded.
pub async fn users(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<AdminUser>>> {
    user.require_admin()?;
    let rows: Vec<UserRow> = sqlx::query_as(USERS_SQL)
        .bind(None::<i64>)
        .fetch_all(&state.db)
        .await?;
    rows.into_iter()
        .map(AdminUser::try_from)
        .collect::<AppResult<_>>()
        .map(Json)
}

/// Starts an account change: takes the lock and checks the caller is still
/// an active admin, and that `target` isn't the caller.
async fn begin_change<'a>(
    state: &'a AppState,
    user: &AuthUser,
    target: i64,
    own: &'static str,
) -> AppResult<sqlx::Transaction<'a, sqlx::Postgres>> {
    require_admin_session(user)?;
    if target == user.id {
        return Err(AppError::BadRequest(own.into()));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ACCOUNTS_LOCK)
        .execute(&mut *tx)
        .await?;
    let still_admin: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM users \
         WHERE id = $1 AND role = 'admin' AND disabled_at IS NULL)",
    )
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    if !still_admin {
        return Err(AppError::Forbidden);
    }
    Ok(tx)
}

/// `PATCH /admin/users/{id}`: role, disabled and storage limit. Disabling
/// signs the account out everywhere; its keys are refused until enabled.
pub async fn update_user(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UserUpdate>,
) -> AppResult<Json<AdminUser>> {
    let mut tx = begin_change(
        &state,
        &user,
        id,
        "you can't change your own role or disable your own account",
    )
    .await?;
    let quota = match req.quota_bytes {
        Some(Some(q)) => Some(
            i64::try_from(q).map_err(|_| AppError::BadRequest("that limit is too big".into()))?,
        ),
        _ => None,
    };
    let done = sqlx::query(
        "UPDATE users SET \
             role = coalesce($2, role), \
             disabled_at = CASE WHEN $3::boolean IS NULL THEN disabled_at \
                                WHEN $3 THEN coalesce(disabled_at, now()) \
                                ELSE NULL END, \
             quota_bytes = CASE WHEN $4 THEN $5 ELSE quota_bytes END \
         WHERE id = $1",
    )
    .bind(id)
    .bind(req.role.map(Role::as_str))
    .bind(req.disabled)
    .bind(req.quota_bytes.is_some())
    .bind(quota)
    .execute(&mut *tx)
    .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if req.disabled == Some(true) {
        sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let row: UserRow = sqlx::query_as(USERS_SQL)
        .bind(Some(id))
        .fetch_one(&state.db)
        .await?;
    AdminUser::try_from(row).map(Json)
}

/// `POST /admin/users/{id}/password`: for someone who forgot theirs. Signs
/// the account out everywhere; its API keys keep working.
pub async fn reset_password(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<PasswordReset>,
) -> AppResult<StatusCode> {
    require_admin_session(&user)?;
    auth::validate_password(&req.new)?;
    let hash = hash_password(req.new).await?;
    let mut tx = begin_change(
        &state,
        &user,
        id,
        "change your own password under Account instead",
    )
    .await?;
    let done = sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.guards.account.clear(&format!("#{id}"));
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /admin/users/{id}`: removes the account, its sessions, keys,
/// votes and saved searches. Its posts stay, without an uploader.
pub async fn delete_user(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let mut tx = begin_change(&state, &user, id, "you can't delete your own account").await?;
    // Votes go with the account, so take them off the scores first.
    sqlx::query(
        "UPDATE posts p SET score = p.score - v.value \
         FROM post_votes v WHERE v.post_id = p.id AND v.user_id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let done = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::Method;
    use serde_json::json;

    use crate::testing;

    #[tokio::test]
    async fn admins_manage_settings_and_accounts() {
        let Some(app) = testing::app_with(|c| c.allow_registration = false).await else {
            return;
        };
        let admin = app.register("admin").await;

        // Settings start from the environment and apply at once.
        let r = app.get("/api/v1/admin/settings", &admin).await;
        assert_eq!(
            r.json(),
            json!({
                "name": "StarBooru",
                "allow_registration": false,
                "trash_days": 0,
                "auto_meta_tags": false
            })
        );
        let put = |token: &str, body: serde_json::Value| {
            let (app, token) = (&app, token.to_owned());
            async move {
                app.call(
                    Method::PUT,
                    "/api/v1/admin/settings",
                    Some(&token),
                    Some(body),
                )
                .await
            }
        };
        let r = put(
            &admin,
            json!({
                "name": "  Home   booru ",
                "allow_registration": true,
                "trash_days": 30,
                "auto_meta_tags": false
            }),
        )
        .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        assert_eq!(r.json()["name"], "Home booru");
        let info = app
            .call(Method::GET, "/api/v1/info", None, None)
            .await
            .json();
        assert_eq!(info["name"], "Home booru");
        assert_eq!(info["registration_open"], true);
        let bad = put(
            &admin,
            json!({ "name": "", "allow_registration": true, "trash_days": 0 }),
        )
        .await;
        assert_eq!(bad.status, 400);

        // Registration is open now.
        let user = app.register("someone").await;
        for path in [
            "/api/v1/admin/stats",
            "/api/v1/admin/users",
            "/api/v1/admin/settings",
        ] {
            assert_eq!(app.get(path, &user).await.status, 403, "{path}");
        }
        let r = put(
            &user,
            json!({ "name": "mine", "allow_registration": true, "trash_days": 0 }),
        )
        .await;
        assert_eq!(r.status, 403);

        // Stats.
        app.upload_png(&user, 1, "cat").await;
        let gone = app.upload_png(&user, 2, "cat").await;
        app.call(
            Method::DELETE,
            &format!("/api/v1/posts/{gone}"),
            Some(&user),
            None,
        )
        .await;
        let stats = app.get("/api/v1/admin/stats", &admin).await.json();
        assert_eq!(stats["users"], 2);
        assert_eq!(stats["images"], 1);
        assert_eq!(stats["bin_posts"], 1);
        assert_eq!(stats["tags"], 1);
        assert!(stats["media_bytes"].as_u64().unwrap() > stats["bin_bytes"].as_u64().unwrap());
        assert!(stats["database_bytes"].as_u64().unwrap() > 0);
        let days = stats["uploads_by_day"].as_array().unwrap();
        assert_eq!(days.len(), 30);
        assert_eq!(days[29]["posts"], 2);
        assert_eq!(days[0]["posts"], 0);

        // Accounts, with usage.
        let users = app.get("/api/v1/admin/users", &admin).await.json();
        assert_eq!(users[0]["username"], "admin");
        assert_eq!(users[1]["username"], "someone");
        assert_eq!(users[1]["posts"], 2);
        assert!(users[1]["last_active"].is_string());
        let uid = users[1]["id"].as_i64().unwrap();
        let aid = users[0]["id"].as_i64().unwrap();
        let path = format!("/api/v1/admin/users/{uid}");
        let patch = |token: &str, path: &str, body: serde_json::Value| {
            let (app, token, path) = (&app, token.to_owned(), path.to_owned());
            async move {
                app.call(Method::PATCH, &path, Some(&token), Some(body))
                    .await
            }
        };

        // A storage limit refuses what would go past it.
        let used = users[1]["bytes"].as_u64().unwrap();
        let r = patch(&admin, &path, json!({ "quota_bytes": used + 10 })).await;
        assert_eq!(r.json()["quota_bytes"], used + 10);
        let r = app.upload(&user, &testing::png(40, 40, 9), &[]).await;
        assert_eq!(r.status, 403);
        assert_eq!(r.json()["code"], "quota_exceeded");
        let usage = app.get("/api/v1/auth/usage", &user).await.json();
        assert_eq!(
            usage,
            json!({ "posts": 2, "bytes": used, "quota_bytes": used + 10 })
        );
        let r = patch(&admin, &path, json!({ "quota_bytes": null })).await;
        assert!(r.json()["quota_bytes"].is_null());
        app.upload_png(&user, 9, "").await;

        // No changing yourself; nobody else's admin page.
        let me = format!("/api/v1/admin/users/{aid}");
        for body in [json!({ "role": "user" }), json!({ "disabled": true })] {
            assert_eq!(patch(&admin, &me, body).await.status, 400);
        }
        assert_eq!(
            app.call(Method::DELETE, &me, Some(&admin), None)
                .await
                .status,
            400
        );
        assert_eq!(
            patch(&user, &me, json!({ "role": "user" })).await.status,
            403
        );
        assert_eq!(
            patch(
                &admin,
                "/api/v1/admin/users/999999",
                json!({ "disabled": true })
            )
            .await
            .status,
            404
        );

        // API keys may look but not change.
        let key = app
            .call(
                Method::POST,
                "/api/v1/keys",
                Some(&admin),
                Some(json!({ "name": "stats" })),
            )
            .await
            .json()["token"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(app.get("/api/v1/admin/stats", &key).await.status, 200);
        assert_eq!(
            patch(&key, &path, json!({ "disabled": true })).await.status,
            403
        );

        // Disabling signs out and blocks sign-in until enabled again.
        let r = patch(&admin, &path, json!({ "disabled": true })).await;
        assert_eq!(r.json()["disabled"], true);
        assert_eq!(app.get("/api/v1/auth/me", &user).await.status, 401);
        let login = || {
            app.call(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "username": "someone", "password": testing::PASSWORD })),
            )
        };
        let r = login().await;
        assert_eq!(r.status, 403);
        assert!(r.json()["message"].as_str().unwrap().contains("disabled"));
        patch(&admin, &path, json!({ "disabled": false })).await;
        assert_eq!(login().await.status, 200);

        // A promoted admin can manage; a password reset signs them out.
        let r = patch(&admin, &path, json!({ "role": "admin" })).await;
        assert_eq!(r.json()["role"], "admin");
        let second = login().await.json()["token"].as_str().unwrap().to_owned();
        assert_eq!(app.get("/api/v1/admin/users", &second).await.status, 200);
        let reset = format!("{path}/password");
        let r = app
            .call(
                Method::POST,
                &reset,
                Some(&admin),
                Some(json!({ "new": "short" })),
            )
            .await;
        assert_eq!(r.status, 400);
        let r = app
            .call(
                Method::POST,
                &reset,
                Some(&admin),
                Some(json!({ "new": "a brand new password" })),
            )
            .await;
        assert_eq!(r.status, 204);
        assert_eq!(app.get("/api/v1/auth/me", &second).await.status, 401);
        assert_eq!(login().await.status, 401);

        // Deleting keeps the posts and takes the votes off the scores.
        let post = app.upload_png(&admin, 20, "").await;
        let token = app
            .call(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "username": "someone", "password": "a brand new password" })),
            )
            .await
            .json()["token"]
            .as_str()
            .unwrap()
            .to_owned();
        app.call(
            Method::PUT,
            &format!("/api/v1/posts/{post}/vote"),
            Some(&token),
            Some(json!({ "value": 1 })),
        )
        .await;
        assert_eq!(
            app.call(Method::DELETE, &path, Some(&admin), None)
                .await
                .status,
            204
        );
        let p = app
            .get(&format!("/api/v1/posts/{post}"), &admin)
            .await
            .json();
        assert_eq!(p["score"], 0);
        let users = app.get("/api/v1/admin/users", &admin).await.json();
        assert_eq!(users.as_array().unwrap().len(), 1);
        let stats = app.get("/api/v1/admin/stats", &admin).await.json();
        assert_eq!(stats["images"], 3);

        app.finish().await;
    }
}
