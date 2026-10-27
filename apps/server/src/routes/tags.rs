use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use starbooru_core::{Tag, TagCategory, TagUpdate, normalize_tag};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MAX_TAG_LIMIT: u32 = 100;

#[derive(Deserialize)]
pub struct TagQuery {
    #[serde(default)]
    q: String,
    limit: Option<u32>,
}

type TagRow = (i64, String, i16, i32);

fn to_tag((id, name, category, post_count): TagRow) -> Tag {
    Tag {
        id,
        name,
        category: TagCategory::from_db(category).unwrap_or_default(),
        post_count,
    }
}

/// Escapes `%`, `_` and `\` for a `LIKE ... ESCAPE '\'` prefix match.
fn like_prefix(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 1);
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// `GET /tags?q=<prefix>&limit=`: autocomplete, most used first.
pub async fn list(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<TagQuery>,
) -> AppResult<Json<Vec<Tag>>> {
    let limit = i64::from(q.limit.unwrap_or(20).clamp(1, MAX_TAG_LIMIT));
    let prefix = q.q.trim().trim_start_matches('-');
    let rows: Vec<TagRow> = if prefix.is_empty() {
        sqlx::query_as(
            "SELECT id, name, category, post_count FROM tags \
             WHERE post_count > 0 ORDER BY post_count DESC, name LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&state.db)
        .await?
    } else {
        let Ok(prefix) = normalize_tag(prefix) else {
            return Ok(Json(Vec::new()));
        };
        sqlx::query_as(
            "SELECT id, name, category, post_count FROM tags \
             WHERE name LIKE $1 ESCAPE '\\' ORDER BY post_count DESC, name LIMIT $2",
        )
        .bind(like_prefix(&prefix))
        .bind(limit)
        .fetch_all(&state.db)
        .await?
    };
    Ok(Json(rows.into_iter().map(to_tag).collect()))
}

pub async fn update(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(name): Path<String>,
    Json(update): Json<TagUpdate>,
) -> AppResult<Json<Tag>> {
    let name = normalize_tag(&name)?;
    let row: TagRow = sqlx::query_as(
        "UPDATE tags SET category = $2 WHERE name = $1 \
         RETURNING id, name, category, post_count",
    )
    .bind(&name)
    .bind(update.category.to_db())
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(to_tag(row)))
}

#[cfg(test)]
mod tests {
    use super::like_prefix;

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(like_prefix("blue_sk"), "blue\\_sk%");
        assert_eq!(like_prefix("100%"), "100\\%%");
        assert_eq!(like_prefix("a\\b"), "a\\\\b%");
    }
}
