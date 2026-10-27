use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use chrono::{DateTime, Utc};
use starbooru_core::{
    AuthResponse, Credentials, PasswordChange, RevokeResult, Role, SessionInfo, User,
    describe_user_agent,
};

use crate::auth::{
    self, AuthUser, SESSION_DAYS, hash_password, hash_token, new_token, verify_password,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

type UserRow = (i64, String, String, DateTime<Utc>);

fn to_user((id, username, role, created_at): UserRow) -> AppResult<User> {
    Ok(User {
        id,
        username,
        role: Role::from_db(&role).ok_or_else(|| AppError::internal("bad role in db"))?,
        created_at,
    })
}

/// "Firefox on Linux" for the request's User-Agent. The header itself is
/// not kept.
fn client_label(headers: &HeaderMap) -> String {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    describe_user_agent(ua)
}

async fn start_session(
    state: &AppState,
    user: User,
    headers: &HeaderMap,
) -> AppResult<Json<AuthResponse>> {
    let token = new_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at, client) \
         VALUES ($1, $2, now() + make_interval(days => $3), $4)",
    )
    .bind(hash_token(&token))
    .bind(user.id)
    .bind(SESSION_DAYS as i32)
    .bind(client_label(headers))
    .execute(&state.db)
    .await?;
    Ok(Json(AuthResponse { token, user }))
}

/// The first account ever created becomes admin; later ones need
/// `STARBOORU_ALLOW_REGISTRATION=true`.
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(creds): Json<Credentials>,
) -> AppResult<Json<AuthResponse>> {
    let username = creds.username.trim().to_owned();
    auth::validate_username(&username)?;
    auth::validate_password(&creds.password)?;
    // Cheap early refusal so closed instances never pay for a password hash.
    if !state.config.allow_registration {
        let has_users: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
            .fetch_one(&state.db)
            .await?;
        if has_users {
            return Err(AppError::Forbidden);
        }
    }
    let password_hash = hash_password(creds.password).await?;

    let mut tx = state.db.begin().await?;
    // Serialise registrations so two "first users" cannot both become admin.
    sqlx::query("SELECT pg_advisory_xact_lock(7275)")
        .execute(&mut *tx)
        .await?;
    let has_users: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&mut *tx)
        .await?;
    if has_users && !state.config.allow_registration {
        return Err(AppError::Forbidden);
    }
    let role = if has_users { Role::User } else { Role::Admin };
    let row: UserRow = sqlx::query_as(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, $3) \
         RETURNING id, username, role, created_at",
    )
    .bind(&username)
    .bind(&password_hash)
    .bind(role.as_str())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::Conflict {
            code: "username_taken",
            message: "that username is taken".into(),
        },
        other => other,
    })?;
    tx.commit().await?;
    start_session(&state, to_user(row)?, &headers).await
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(creds): Json<Credentials>,
) -> AppResult<Json<AuthResponse>> {
    let row: Option<(i64, String, String, DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT id, username, role, created_at, password_hash FROM users \
         WHERE lower(username) = lower($1)",
    )
    .bind(creds.username.trim())
    .fetch_optional(&state.db)
    .await?;
    let stored = row.as_ref().map(|r| r.4.clone());
    if !verify_password(creds.password, stored).await? {
        return Err(AppError::Unauthorized);
    }
    let Some((id, username, role, created_at, _)) = row else {
        return Err(AppError::Unauthorized);
    };
    sqlx::query("DELETE FROM sessions WHERE expires_at < now()")
        .execute(&state.db)
        .await?;
    start_session(&state, to_user((id, username, role, created_at))?, &headers).await
}

pub async fn logout(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(&user.token_hash)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<User>> {
    let row: UserRow =
        sqlx::query_as("SELECT id, username, role, created_at FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(to_user(row)?))
}

/// `PUT /auth/password`: needs the current password. Signs out every other
/// session.
pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    Json(change): Json<PasswordChange>,
) -> AppResult<StatusCode> {
    auth::validate_password(&change.new)?;
    let stored: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&state.db)
        .await?;
    if !verify_password(change.current, Some(stored)).await? {
        return Err(AppError::BadRequest("the current password is wrong".into()));
    }
    let hash = hash_password(change.new).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(user.id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND id <> $2")
        .bind(user.id)
        .bind(user.session_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /auth/sessions`: the caller's live sessions, this one first, then
/// the most recently used.
pub async fn sessions(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<SessionInfo>>> {
    type Row = (i64, String, DateTime<Utc>, DateTime<Utc>, DateTime<Utc>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, client, created_at, last_used_at, expires_at FROM sessions \
         WHERE user_id = $1 AND expires_at > now() \
         ORDER BY id = $2 DESC, last_used_at DESC, id DESC",
    )
    .bind(user.id)
    .bind(user.session_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, client, created_at, last_used_at, expires_at)| SessionInfo {
                    id,
                    client,
                    created_at,
                    last_used_at,
                    expires_at,
                    current: id == user.session_id,
                },
            )
            .collect(),
    ))
}

/// `DELETE /auth/sessions/{id}`: signs one of the caller's sessions out.
pub async fn revoke_session(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let done = sqlx::query("DELETE FROM sessions WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /auth/sessions`: signs out everywhere but here.
pub async fn revoke_other_sessions(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<RevokeResult>> {
    let done = sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND id <> $2")
        .bind(user.id)
        .bind(user.session_id)
        .execute(&state.db)
        .await?;
    Ok(Json(RevokeResult {
        revoked: done.rows_affected(),
    }))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use serde_json::json;

    use super::*;
    use crate::testing::{self, TestApp};

    async fn login_as(app: &TestApp, username: &str, password: &str, ua: &str) -> (u16, String) {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::USER_AGENT, ua)
            .body(Body::from(
                json!({ "username": username, "password": password }).to_string(),
            ))
            .unwrap();
        let r = app.send(req).await;
        let token = r.json()["token"].as_str().unwrap_or_default().to_owned();
        (r.status.as_u16(), token)
    }

    #[tokio::test]
    async fn changes_password_and_manages_sessions() {
        let Some(app) = testing::app().await else {
            return;
        };
        let first = app.register("alice").await;
        let other = app.register("bob").await;
        let password = testing::PASSWORD;
        let (status, firefox) = login_as(
            &app,
            "alice",
            password,
            "Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0",
        )
        .await;
        assert_eq!(status, 200);
        let (_, desktop) = login_as(&app, "alice", password, "StarBooru-app/0.1.0 (windows)").await;

        let list = app.get("/api/v1/auth/sessions", &firefox).await.json();
        let list = list.as_array().unwrap();
        assert_eq!(list.len(), 3, "bob's session is not listed");
        assert_eq!(list[0]["current"], true, "this session comes first");
        let clients: Vec<&str> = list.iter().map(|s| s["client"].as_str().unwrap()).collect();
        assert!(clients.contains(&"Firefox on Linux"), "{clients:?}");
        assert!(clients.contains(&"StarBooru app on Windows"), "{clients:?}");
        let current: Vec<&serde_json::Value> =
            list.iter().filter(|s| s["current"] == true).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0]["client"], "Firefox on Linux");
        let desktop_id = list
            .iter()
            .find(|s| s["client"] == "StarBooru app on Windows")
            .unwrap()["id"]
            .as_i64()
            .unwrap();

        // Someone else's session is not yours to revoke.
        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/auth/sessions/{desktop_id}"),
                Some(&other),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/auth/sessions/{desktop_id}"),
                Some(&firefox),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        assert_eq!(
            app.get("/api/v1/auth/me", &desktop).await.status,
            StatusCode::UNAUTHORIZED
        );

        // Password change: wrong current password, too short, then right.
        let change = |current: &str, new: &str| json!({ "current": current, "new": new });
        let put = |token: String, body: serde_json::Value| {
            let app = &app;
            async move {
                app.call(
                    Method::PUT,
                    "/api/v1/auth/password",
                    Some(&token),
                    Some(body),
                )
                .await
                .status
            }
        };
        assert_eq!(
            put(firefox.clone(), change("wrong-password", "new-password-1")).await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(firefox.clone(), change(password, "short")).await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(firefox.clone(), change(password, "new-password-1")).await,
            StatusCode::NO_CONTENT
        );
        // Other sessions are signed out; this one and bob's stay.
        assert_eq!(
            app.get("/api/v1/auth/me", &first).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.get("/api/v1/auth/me", &firefox).await.status,
            StatusCode::OK
        );
        assert_eq!(
            app.get("/api/v1/auth/me", &other).await.status,
            StatusCode::OK
        );
        assert_eq!(login_as(&app, "alice", password, "x").await.0, 401);
        let (status, again) = login_as(&app, "alice", "new-password-1", "curl/8.0").await;
        assert_eq!(status, 200);

        // Sign out everywhere else.
        let r = app
            .call(Method::DELETE, "/api/v1/auth/sessions", Some(&again), None)
            .await;
        assert_eq!(r.json(), json!({ "revoked": 1 }));
        let list = app.get("/api/v1/auth/sessions", &again).await.json();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["client"], "curl");
        assert_eq!(list[0]["current"], true);
        assert_eq!(
            app.get("/api/v1/auth/me", &firefox).await.status,
            StatusCode::UNAUTHORIZED
        );

        // A stale session is touched on use.
        sqlx::query("UPDATE sessions SET last_used_at = now() - interval '1 day'")
            .execute(app.db())
            .await
            .unwrap();
        app.get("/api/v1/auth/me", &again).await;
        let fresh: bool = sqlx::query_scalar(
            "SELECT last_used_at > now() - interval '1 minute' FROM sessions WHERE client = 'curl'",
        )
        .fetch_one(app.db())
        .await
        .unwrap();
        assert!(fresh);
        app.finish().await;
    }
}
