//! Saved searches: each user's own list.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use starbooru_core::saved::MAX_SAVED_SEARCHES;
use starbooru_core::{SaveSearchRequest, SavedSearch, clean_saved_search};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(FromRow)]
struct Row {
    id: i64,
    name: String,
    query: String,
    created_at: DateTime<Utc>,
}

impl From<Row> for SavedSearch {
    fn from(r: Row) -> Self {
        SavedSearch {
            id: r.id,
            name: r.name,
            query: r.query,
            created_at: r.created_at,
        }
    }
}

/// `GET /searches`: the caller's saved searches, by name.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<SavedSearch>>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, name, query, created_at FROM saved_searches \
         WHERE user_id = $1 ORDER BY lower(name), id",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(SavedSearch::from).collect()))
}

/// `POST /searches`: 409 `name_taken` if the caller already has that name.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<SaveSearchRequest>,
) -> AppResult<Json<SavedSearch>> {
    let (name, query) = clean_saved_search(&req.name, &req.query)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let mut tx = state.db.begin().await?;
    // Serialises one user's saves, so the limit holds.
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM saved_searches WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= i64::try_from(MAX_SAVED_SEARCHES).unwrap_or(i64::MAX) {
        return Err(AppError::BadRequest(format!(
            "you can keep at most {MAX_SAVED_SEARCHES} saved searches"
        )));
    }
    let row: Row = sqlx::query_as(
        "INSERT INTO saved_searches (user_id, name, query) VALUES ($1, $2, $3) \
         RETURNING id, name, query, created_at",
    )
    .bind(user.id)
    .bind(&name)
    .bind(&query)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::Conflict {
            code: "name_taken",
            message: format!("you already have a saved search called `{name}`"),
        },
        other => other,
    })?;
    tx.commit().await?;
    Ok(Json(row.into()))
}

/// `DELETE /searches/{id}`: only the owner's own.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let done = sqlx::query("DELETE FROM saved_searches WHERE id = $1 AND user_id = $2")
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
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    use crate::testing;

    #[tokio::test]
    async fn saves_lists_and_deletes() {
        let Some(app) = testing::app().await else {
            return;
        };
        let alice = app.register("alice").await;
        let bob = app.register("bob").await;
        let save = |name: &str, query: &str| Some(json!({ "name": name, "query": query }));

        let r = app
            .call(
                Method::POST,
                "/api/v1/searches",
                Some(&alice),
                save(" Skies ", "blue_sky  -cloud"),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let id = r.json()["id"].as_i64().unwrap();
        assert_eq!(r.json()["query"], "blue_sky -cloud");
        app.call(
            Method::POST,
            "/api/v1/searches",
            Some(&alice),
            save("animals", "cat"),
        )
        .await;

        let dup = app
            .call(
                Method::POST,
                "/api/v1/searches",
                Some(&alice),
                save("SKIES", "x"),
            )
            .await;
        assert_eq!(
            (dup.status, dup.json()["code"].as_str()),
            (StatusCode::CONFLICT, Some("name_taken"))
        );
        let bad = app
            .call(
                Method::POST,
                "/api/v1/searches",
                Some(&alice),
                save("", "x"),
            )
            .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        // Bob may use the same name; lists are per user.
        let r = app
            .call(
                Method::POST,
                "/api/v1/searches",
                Some(&bob),
                save("skies", "sky"),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);

        let list = app.get("/api/v1/searches", &alice).await.json();
        let names: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["animals", "Skies"]);
        assert_eq!(
            app.get("/api/v1/searches", &bob)
                .await
                .json()
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let path = format!("/api/v1/searches/{id}");
        let theirs = app.call(Method::DELETE, &path, Some(&bob), None).await;
        assert_eq!(theirs.status, StatusCode::NOT_FOUND);
        let mine = app.call(Method::DELETE, &path, Some(&alice), None).await;
        assert_eq!(mine.status, StatusCode::NO_CONTENT);
        assert_eq!(
            app.get("/api/v1/searches", &alice)
                .await
                .json()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let none = app.call(Method::GET, "/api/v1/searches", None, None).await;
        assert_eq!(none.status, StatusCode::UNAUTHORIZED);
        app.finish().await;
    }
}
