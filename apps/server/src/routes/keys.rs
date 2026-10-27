//! API keys: each user's long-lived tokens for scripts and other tools.
//! Managing them needs a sign-in session, so one key can't mint another.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use starbooru_core::keys::{API_KEY_SHOWN_CHARS, MAX_API_KEYS};
use starbooru_core::{ApiKey, CreatedApiKey, NewApiKey, clean_api_key};

use crate::auth::{AuthUser, hash_token, new_api_key};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(FromRow)]
struct Row {
    id: i64,
    name: String,
    prefix: String,
    read_only: bool,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
}

impl From<Row> for ApiKey {
    fn from(r: Row) -> Self {
        ApiKey {
            id: r.id,
            name: r.name,
            prefix: r.prefix,
            read_only: r.read_only,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
            expires_at: r.expires_at,
        }
    }
}

/// `GET /keys`: the caller's keys, newest first. Expired keys are listed
/// until removed, so their owner sees why a script stopped working.
pub async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<ApiKey>>> {
    user.session_id()?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, name, prefix, read_only, created_at, last_used_at, expires_at \
         FROM api_keys WHERE user_id = $1 ORDER BY id DESC",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(ApiKey::from).collect()))
}

/// `POST /keys`: the token is in the answer and nowhere else.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<NewApiKey>,
) -> AppResult<Json<CreatedApiKey>> {
    user.session_id()?;
    let name = clean_api_key(&req.name, req.expires_days)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let days = req
        .expires_days
        .map(i32::try_from)
        .transpose()
        .map_err(AppError::internal)?;
    let token = new_api_key();
    let prefix: String = token.chars().take(API_KEY_SHOWN_CHARS).collect();
    let mut tx = state.db.begin().await?;
    // Serialises one user's changes, so the limit holds.
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM api_keys WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= i64::try_from(MAX_API_KEYS).unwrap_or(i64::MAX) {
        return Err(AppError::BadRequest(format!(
            "you can have at most {MAX_API_KEYS} API keys; remove one first"
        )));
    }
    let row: Row = sqlx::query_as(
        "INSERT INTO api_keys (user_id, name, token_hash, prefix, read_only, expires_at) \
         VALUES ($1, $2, $3, $4, $5, now() + make_interval(days => $6)) \
         RETURNING id, name, prefix, read_only, created_at, last_used_at, expires_at",
    )
    .bind(user.id)
    .bind(&name)
    .bind(hash_token(&token))
    .bind(&prefix)
    .bind(req.read_only)
    .bind(days)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(CreatedApiKey {
        key: row.into(),
        token,
    }))
}

/// `DELETE /keys/{id}`: the key stops working at once.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    user.session_id()?;
    let done = sqlx::query("DELETE FROM api_keys WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::Method;
    use serde_json::json;

    use crate::testing;

    #[tokio::test]
    async fn keys_act_as_their_owner_within_limits() {
        let Some(app) = testing::app().await else {
            return;
        };
        let alice = app.register("alice").await;
        let bob = app.register("bob").await;
        let create = |token: String, body: serde_json::Value| {
            let app = &app;
            async move {
                app.call(Method::POST, "/api/v1/keys", Some(&token), Some(body))
                    .await
            }
        };

        // Bad input.
        let r = create(alice.clone(), json!({ "name": "  " })).await;
        assert_eq!(r.status, 400);
        let r = create(alice.clone(), json!({ "name": "x", "expires_days": 0 })).await;
        assert_eq!(r.status, 400);

        // A full key acts as alice.
        let r = create(alice.clone(), json!({ "name": " backup   script " })).await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        let full = r.json()["token"].as_str().unwrap().to_owned();
        let full_id = r.json()["key"]["id"].as_i64().unwrap();
        assert!(full.starts_with("sbk_"));
        assert_eq!(r.json()["key"]["name"], "backup script");
        assert_eq!(r.json()["key"]["prefix"], full[..12]);
        assert!(r.json()["key"]["expires_at"].is_null());
        assert!(r.json()["key"]["last_used_at"].is_null());
        let r = app.get("/api/v1/auth/me", &full).await;
        assert_eq!(r.json()["username"], "alice");
        let id = app.upload_png(&full, 1, "cat").await;
        assert!(id > 0);

        // Keys can't manage sign-ins, the password or keys.
        for (method, path, body) in [
            (Method::GET, "/api/v1/keys", None),
            (
                Method::POST,
                "/api/v1/keys",
                Some(json!({ "name": "child" })),
            ),
            (Method::GET, "/api/v1/auth/sessions", None),
            (Method::DELETE, "/api/v1/auth/sessions", None),
            (Method::POST, "/api/v1/auth/logout", None),
            (
                Method::PUT,
                "/api/v1/auth/password",
                Some(json!({ "current": testing::PASSWORD, "new": "another password" })),
            ),
        ] {
            let r = app.call(method, path, Some(&full), body).await;
            assert_eq!(r.status, 403, "{path}");
            assert_eq!(r.json()["code"], "forbidden");
        }

        // A read-only key that expires.
        let r = create(
            alice.clone(),
            json!({ "name": "viewer", "read_only": true, "expires_days": 30 }),
        )
        .await;
        let viewer = r.json()["token"].as_str().unwrap().to_owned();
        assert_eq!(r.json()["key"]["read_only"], true);
        assert!(r.json()["key"]["expires_at"].is_string());
        assert_eq!(
            app.get(&format!("/api/v1/posts/{id}"), &viewer)
                .await
                .status,
            200
        );
        let r = app
            .call(
                Method::PUT,
                &format!("/api/v1/posts/{id}/vote"),
                Some(&viewer),
                Some(json!({ "value": 1 })),
            )
            .await;
        assert_eq!(r.status, 403);
        assert_eq!(r.json()["message"], "this API key is read-only");

        // The list shows both, newest first, with use recorded.
        let r = app.get("/api/v1/keys", &alice).await;
        let list = r.json();
        assert_eq!(list.as_array().unwrap().len(), 2);
        assert_eq!(list[0]["name"], "viewer");
        assert!(list[1]["last_used_at"].is_string());
        assert!(!String::from_utf8_lossy(&r.body).contains(&full));

        // Expired keys stop working.
        sqlx::query(
            "UPDATE api_keys SET expires_at = now() - interval '1 second' WHERE name = 'viewer'",
        )
        .execute(app.db())
        .await
        .unwrap();
        assert_eq!(app.get("/api/v1/auth/me", &viewer).await.status, 401);

        // Only the owner removes a key, and it stops working at once.
        let path = format!("/api/v1/keys/{full_id}");
        assert_eq!(
            app.call(Method::DELETE, &path, Some(&bob), None)
                .await
                .status,
            404
        );
        assert_eq!(app.get("/api/v1/auth/me", &full).await.status, 200);
        assert_eq!(
            app.call(Method::DELETE, &path, Some(&alice), None)
                .await
                .status,
            204
        );
        assert_eq!(app.get("/api/v1/auth/me", &full).await.status, 401);
        assert_eq!(app.get("/api/v1/keys", &bob).await.json(), json!([]));

        // At most MAX_API_KEYS each.
        sqlx::query(
            "INSERT INTO api_keys (user_id, name, token_hash, prefix) \
             SELECT u.id, 'k' || n, sha256(('k' || n)::bytea), 'sbk_' \
             FROM users u, generate_series(1, 49) n WHERE u.username = 'bob'",
        )
        .execute(app.db())
        .await
        .unwrap();
        assert_eq!(
            create(bob.clone(), json!({ "name": "last" })).await.status,
            200
        );
        assert_eq!(
            create(bob.clone(), json!({ "name": "one too many" }))
                .await
                .status,
            400
        );

        app.finish().await;
    }
}
