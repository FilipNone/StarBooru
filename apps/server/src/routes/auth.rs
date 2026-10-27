use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use chrono::{DateTime, Utc};
use starbooru_core::{
    AuthResponse, Credentials, PasswordChange, RevokeResult, Role, SessionInfo, Usage, User,
    describe_user_agent,
};

use crate::auth::{
    self, AuthUser, SESSION_DAYS, hash_password, hash_token, new_token, verify_password,
};
use crate::error::{AppError, AppResult};
use crate::settings;
use crate::state::AppState;
use crate::throttle::{ClientAddr, account_key};

pub(super) type UserRow = (i64, String, String, DateTime<Utc>);
/// A user row, plus the password hash (none for provider-only accounts)
/// and whether the account is disabled.
type LoginRow = (i64, String, String, DateTime<Utc>, Option<String>, bool);

pub(super) fn to_user((id, username, role, created_at): UserRow) -> AppResult<User> {
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

pub(super) async fn start_session(
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
    ClientAddr(addr): ClientAddr,
    headers: HeaderMap,
    Json(creds): Json<Credentials>,
) -> AppResult<Json<AuthResponse>> {
    if let Some(a) = &addr {
        let now = Utc::now();
        state
            .guards
            .signup
            .check(a, now)
            .map_err(|retry_after| AppError::TooManyRequests { retry_after })?;
        state.guards.signup.hit(a, now);
    }
    let username = creds.username.trim().to_owned();
    auth::validate_username(&username)?;
    auth::validate_password(&creds.password)?;
    // Cheap early refusal so closed instances never pay for a password hash.
    let open = settings::current(&state).await?.allow_registration;
    if !open {
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
    if has_users && !open {
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

/// Wrong passwords count against the account and the client address (see
/// `throttle`); a locked one answers 429 before the password is checked.
pub async fn login(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    headers: HeaderMap,
    Json(creds): Json<Credentials>,
) -> AppResult<Json<AuthResponse>> {
    let now = Utc::now();
    let account = account_key(&creds.username);
    state.guards.check_login(&account, addr.as_deref(), now)?;
    let row: Option<LoginRow> = sqlx::query_as(
        "SELECT id, username, role, created_at, password_hash, disabled_at IS NOT NULL \
         FROM users WHERE lower(username) = lower($1)",
    )
    .bind(creds.username.trim())
    .fetch_optional(&state.db)
    .await?;
    // Accounts without a password (made by a provider sign-in) fail like an
    // unknown name.
    let stored = row.as_ref().and_then(|r| r.4.clone());
    let verified = verify_password(creds.password, stored).await?;
    let Some((id, username, role, created_at, _, disabled)) = row.filter(|_| verified) else {
        state.guards.failed_login(&account, addr.as_deref(), now);
        return Err(AppError::Unauthorized);
    };
    state.guards.account.clear(&account);
    // Said only after the right password, so it reveals nothing to a guesser.
    if disabled {
        return Err(AppError::Denied(
            "this account is disabled; ask the server's admin",
        ));
    }
    sqlx::query("DELETE FROM sessions WHERE expires_at < now()")
        .execute(&state.db)
        .await?;
    start_session(&state, to_user((id, username, role, created_at))?, &headers).await
}

pub async fn logout(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(user.session_id()?)
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

/// `GET /auth/usage`: what the caller has uploaded, against their limit.
pub async fn usage(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Usage>> {
    let (posts, bytes, quota): (i64, i64, Option<i64>) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM posts WHERE uploader_id = $1), \
                (SELECT coalesce(sum(file_size), 0)::bigint FROM posts WHERE uploader_id = $1), \
                quota_bytes \
         FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(Usage {
        posts: u64::try_from(posts).unwrap_or(0),
        bytes: u64::try_from(bytes).unwrap_or(0),
        quota_bytes: quota.and_then(|q| u64::try_from(q).ok()),
    }))
}

/// `PUT /auth/password`: needs the current password, if there is one.
/// Signs out every other session.
pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    Json(change): Json<PasswordChange>,
) -> AppResult<StatusCode> {
    let session_id = user.session_id()?;
    auth::validate_password(&change.new)?;
    let stored: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&state.db)
            .await?;
    let now = Utc::now();
    let key = format!("#{}", user.id);
    state
        .guards
        .account
        .check(&key, now)
        .map_err(|retry_after| AppError::TooManyRequests { retry_after })?;
    if let Some(stored) = stored
        && !verify_password(change.current, Some(stored)).await?
    {
        state.guards.account.hit(&key, now);
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
        .bind(session_id)
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
    let session_id = user.session_id()?;
    type Row = (i64, String, DateTime<Utc>, DateTime<Utc>, DateTime<Utc>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, client, created_at, last_used_at, expires_at FROM sessions \
         WHERE user_id = $1 AND expires_at > now() \
         ORDER BY id = $2 DESC, last_used_at DESC, id DESC",
    )
    .bind(user.id)
    .bind(session_id)
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
                    current: id == session_id,
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
    user.session_id()?;
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
        .bind(user.session_id()?)
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
    use serde_json::{Value, json};

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

    async fn attempt(
        app: &TestApp,
        path: &str,
        from: &str,
        body: Value,
        token: Option<&str>,
    ) -> testing::Resp {
        let mut req = Request::builder()
            .method(if path.ends_with("password") {
                Method::PUT
            } else {
                Method::POST
            })
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", format!("192.0.2.200, {from}"));
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        app.send(req.body(Body::from(body.to_string())).unwrap())
            .await
    }

    #[tokio::test]
    async fn throttles_password_guessing() {
        let Some(app) = testing::app_with(|c| c.trust_proxy = true).await else {
            return;
        };
        let alice = app.register("alice").await;
        app.register("bob").await;
        let login = "/api/v1/auth/login";
        let guess = |user: &str, password: &str| json!({"username": user, "password": password});

        for _ in 0..10 {
            let r = attempt(
                &app,
                login,
                "198.51.100.1",
                guess("Alice", "wrong-guess"),
                None,
            )
            .await;
            assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        }
        // Locked: even the right password waits, from any address.
        let r = attempt(
            &app,
            login,
            "198.51.100.2",
            guess("alice", testing::PASSWORD),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(r.json()["code"], "rate_limited");
        let wait: u64 = r.headers[header::RETRY_AFTER]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!((1..=15 * 60).contains(&wait));
        // Other accounts are unaffected.
        let r = attempt(
            &app,
            login,
            "198.51.100.1",
            guess("bob", testing::PASSWORD),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::OK);

        // Sign-ups: 10 per address per hour (alice and bob were 2).
        for i in 0..8 {
            let body = guess(&format!("user{i}"), testing::PASSWORD);
            let r = attempt(&app, "/api/v1/auth/register", "203.0.113.7", body, None).await;
            assert_eq!(r.status, StatusCode::OK);
        }
        let body = guess("one_too_many", testing::PASSWORD);
        let r = attempt(
            &app,
            "/api/v1/auth/register",
            "203.0.113.7",
            body.clone(),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
        let r = attempt(&app, "/api/v1/auth/register", "203.0.113.8", body, None).await;
        assert_eq!(r.status, StatusCode::OK);

        // A stolen session can't guess the current password either.
        let change = json!({"current": "wrong-guess", "new": "a-new-password"});
        for _ in 0..10 {
            let r = attempt(
                &app,
                "/api/v1/auth/password",
                "198.51.100.3",
                change.clone(),
                Some(&alice),
            )
            .await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST);
        }
        let change = json!({"current": testing::PASSWORD, "new": "a-new-password"});
        let r = attempt(
            &app,
            "/api/v1/auth/password",
            "198.51.100.3",
            change,
            Some(&alice),
        )
        .await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
        app.finish().await;
    }
}
