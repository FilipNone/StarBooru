use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use starbooru_core::{AuthResponse, Credentials, Role, User};

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

async fn start_session(state: &AppState, user: User) -> AppResult<Json<AuthResponse>> {
    let token = new_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at) \
         VALUES ($1, $2, now() + make_interval(days => $3))",
    )
    .bind(hash_token(&token))
    .bind(user.id)
    .bind(SESSION_DAYS as i32)
    .execute(&state.db)
    .await?;
    Ok(Json(AuthResponse { token, user }))
}

/// The first account ever created becomes admin; later ones need
/// `STARBOORU_ALLOW_REGISTRATION=true`.
pub async fn register(
    State(state): State<AppState>,
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
    start_session(&state, to_user(row)?).await
}

pub async fn login(
    State(state): State<AppState>,
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
    start_session(&state, to_user((id, username, role, created_at))?).await
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
