//! Tags: autocomplete and the tag list, categories, aliases and implications.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::{PgExecutor, Postgres, Transaction};
use starbooru_core::{
    AliasRequest, ImplicationRequest, Role, Tag, TagCategory, TagDetail, TagOrder, TagUpdate,
    normalize_tag,
};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MAX_TAG_LIMIT: u32 = 100;
const MAX_TAG_PAGE: u32 = 10_000;
/// Serialises alias and implication changes, which rewrite many posts.
const TAG_GRAPH_LOCK: i64 = 7276;

#[derive(Deserialize)]
pub struct TagQuery {
    #[serde(default)]
    q: String,
    limit: Option<u32>,
    page: Option<u32>,
    #[serde(default)]
    order: TagOrder,
}

pub(super) type TagRow = (i64, String, i16, i32);

const TAG_COLUMNS: &str = "id, name, category, post_count";

pub(super) fn to_tag((id, name, category, post_count): TagRow) -> Tag {
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

/// `GET /tags?q=<prefix>&limit=&page=&order=count|name`. With a prefix it is
/// the autocomplete list, and aliases match too (returning their tag).
pub async fn list(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<TagQuery>,
) -> AppResult<Json<Vec<Tag>>> {
    let limit = i64::from(q.limit.unwrap_or(20).clamp(1, MAX_TAG_LIMIT));
    let offset = i64::from(q.page.unwrap_or(1).clamp(1, MAX_TAG_PAGE) - 1) * limit;
    let order = match q.order {
        TagOrder::Count => " ORDER BY post_count DESC, name",
        TagOrder::Name => " ORDER BY name",
    };
    let prefix = q.q.trim().trim_start_matches('-');
    let mut qb = sqlx::QueryBuilder::<Postgres>::new("SELECT ");
    if prefix.is_empty() {
        qb.push(TAG_COLUMNS).push(" FROM tags WHERE post_count > 0");
    } else {
        let Ok(prefix) = normalize_tag(prefix) else {
            return Ok(Json(Vec::new()));
        };
        let pattern = like_prefix(&prefix);
        qb.push(TAG_COLUMNS)
            .push(" FROM tags WHERE name LIKE ")
            .push_bind(pattern.clone())
            .push(
                " ESCAPE '\\' UNION SELECT t.id, t.name, t.category, t.post_count \
                   FROM tag_aliases a JOIN tags t ON t.id = a.tag_id WHERE a.alias LIKE ",
            )
            .push_bind(pattern)
            .push(" ESCAPE '\\'");
    }
    qb.push(order)
        .push(" LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let rows: Vec<TagRow> = qb.build_query_as().fetch_all(&state.db).await?;
    Ok(Json(rows.into_iter().map(to_tag).collect()))
}

/// Normalises names, replaces aliases with their tag and removes duplicates,
/// keeping first-seen order.
pub async fn canonical_names(db: impl PgExecutor<'_>, names: &[String]) -> AppResult<Vec<String>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let resolved: Vec<String> = sqlx::query_scalar(
        "SELECT COALESCE(t.name, n.name) \
         FROM unnest($1::text[]) WITH ORDINALITY AS n(name, ord) \
         LEFT JOIN tag_aliases a ON a.alias = n.name \
         LEFT JOIN tags t ON t.id = a.tag_id \
         ORDER BY n.ord",
    )
    .bind(names)
    .fetch_all(db)
    .await?;
    let mut out: Vec<String> = Vec::with_capacity(resolved.len());
    for name in resolved {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Tag ids for a post being tagged with `names`: aliases resolved, missing
/// tags created, implied tags added (transitively).
pub async fn resolve_for_post(
    tx: &mut Transaction<'_, Postgres>,
    names: &[String],
) -> AppResult<Vec<i64>> {
    let names = canonical_names(&mut **tx, names).await?;
    if names.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query("INSERT INTO tags (name) SELECT unnest($1::text[]) ON CONFLICT (name) DO NOTHING")
        .bind(&names)
        .execute(&mut **tx)
        .await?;
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE imp(id) AS ( \
             SELECT id FROM tags WHERE name = ANY($1) \
             UNION SELECT i.implied_id FROM tag_implications i JOIN imp ON i.tag_id = imp.id \
         ) SELECT id FROM imp",
    )
    .bind(&names)
    .fetch_all(&mut **tx)
    .await?)
}

/// The tag and everything it implies, transitively.
async fn closure(tx: &mut Transaction<'_, Postgres>, tag_id: i64) -> AppResult<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE imp(id) AS ( \
             SELECT $1::bigint \
             UNION SELECT i.implied_id FROM tag_implications i JOIN imp ON i.tag_id = imp.id \
         ) SELECT id FROM imp",
    )
    .bind(tag_id)
    .fetch_all(&mut **tx)
    .await?)
}

/// Adds everything `tag_id` implies to the posts that carry it, then
/// recounts the affected tags.
async fn apply_implications(tx: &mut Transaction<'_, Postgres>, tag_id: i64) -> AppResult<()> {
    let ids = closure(tx, tag_id).await?;
    sqlx::query(
        "UPDATE posts SET tag_ids = ARRAY(SELECT DISTINCT unnest(tag_ids || $2::bigint[])), \
         updated_at = now() \
         WHERE tag_ids @> ARRAY[$1::bigint] AND NOT tag_ids @> $2::bigint[]",
    )
    .bind(tag_id)
    .bind(&ids)
    .execute(&mut **tx)
    .await?;
    recount(tx, &ids).await
}

/// Recomputes `post_count` from the posts themselves.
async fn recount(tx: &mut Transaction<'_, Postgres>, ids: &[i64]) -> AppResult<()> {
    sqlx::query(
        "UPDATE tags SET post_count = (SELECT count(*) FROM posts \
         WHERE deleted_at IS NULL AND tag_ids @> ARRAY[tags.id]) WHERE id = ANY($1)",
    )
    .bind(ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Resolves a tag name from a URL (following an alias) to its row.
async fn find(db: impl PgExecutor<'_>, raw: &str) -> AppResult<TagRow> {
    let name = normalize_tag(raw)?;
    sqlx::query_as(
        "SELECT t.id, t.name, t.category, t.post_count FROM tags t \
         WHERE t.name = $1 OR t.id = (SELECT tag_id FROM tag_aliases WHERE alias = $1) \
         ORDER BY t.name = $1 DESC LIMIT 1",
    )
    .bind(&name)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

/// Finds or creates the tag `raw` names (following an alias).
async fn find_or_create(tx: &mut Transaction<'_, Postgres>, raw: &str) -> AppResult<TagRow> {
    let name = normalize_tag(raw)?;
    let names = canonical_names(&mut **tx, std::slice::from_ref(&name)).await?;
    let canonical = names.into_iter().next().unwrap_or(name);
    sqlx::query("INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO NOTHING")
        .bind(&canonical)
        .execute(&mut **tx)
        .await?;
    find(&mut **tx, &canonical).await
}

async fn detail_of(db: &sqlx::PgPool, tag: TagRow) -> AppResult<TagDetail> {
    let id = tag.0;
    let aliases: Vec<String> =
        sqlx::query_scalar("SELECT alias FROM tag_aliases WHERE tag_id = $1 ORDER BY alias")
            .bind(id)
            .fetch_all(db)
            .await?;
    let implies: Vec<TagRow> = sqlx::query_as(
        "SELECT t.id, t.name, t.category, t.post_count FROM tag_implications i \
         JOIN tags t ON t.id = i.implied_id WHERE i.tag_id = $1 ORDER BY t.name",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let implied_by: Vec<TagRow> = sqlx::query_as(
        "SELECT t.id, t.name, t.category, t.post_count FROM tag_implications i \
         JOIN tags t ON t.id = i.tag_id WHERE i.implied_id = $1 ORDER BY t.name",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(TagDetail {
        tag: to_tag(tag),
        aliases,
        implies: implies.into_iter().map(to_tag).collect(),
        implied_by: implied_by.into_iter().map(to_tag).collect(),
    })
}

/// `GET /tags/{name}`: the tag (or the tag an alias points to) with its relations.
pub async fn detail(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(name): Path<String>,
) -> AppResult<Json<TagDetail>> {
    let tag = find(&state.db, &name).await?;
    Ok(Json(detail_of(&state.db, tag).await?))
}

pub async fn update(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(name): Path<String>,
    Json(update): Json<TagUpdate>,
) -> AppResult<Json<Tag>> {
    let id = find(&state.db, &name).await?.0;
    let row: TagRow = sqlx::query_as(
        "UPDATE tags SET category = $2 WHERE id = $1 RETURNING id, name, category, post_count",
    )
    .bind(id)
    .bind(update.category.to_db())
    .fetch_one(&state.db)
    .await?;
    Ok(Json(to_tag(row)))
}

fn require_admin(user: &AuthUser) -> AppResult<()> {
    if user.role == Role::Admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn lock_tag_graph(tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(TAG_GRAPH_LOCK)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// `POST /tags/{name}/aliases {alias}` (admin). If a tag called `alias`
/// exists, it is merged into this one: its posts, aliases and implications move over.
pub async fn add_alias(
    State(state): State<AppState>,
    user: AuthUser,
    Path(name): Path<String>,
    Json(req): Json<AliasRequest>,
) -> AppResult<Json<TagDetail>> {
    require_admin(&user)?;
    let alias = normalize_tag(&req.alias)?;
    let mut tx = state.db.begin().await?;
    lock_tag_graph(&mut tx).await?;
    let target = find_or_create(&mut tx, &name).await?;
    let target_id = target.0;
    if alias == target.1 {
        return Err(AppError::BadRequest(
            "a tag can't be an alias of itself".into(),
        ));
    }

    let old: Option<i64> = sqlx::query_scalar("SELECT id FROM tags WHERE name = $1")
        .bind(&alias)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(old_id) = old {
        sqlx::query(
            "UPDATE posts SET tag_ids = ARRAY(SELECT DISTINCT unnest( \
                 array_replace(tag_ids, $1::bigint, $2::bigint))), updated_at = now() \
             WHERE tag_ids @> ARRAY[$1::bigint]",
        )
        .bind(old_id)
        .bind(target_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE tag_aliases SET tag_id = $2 WHERE tag_id = $1")
            .bind(old_id)
            .bind(target_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO tag_implications (tag_id, implied_id) \
             SELECT $2, implied_id FROM tag_implications WHERE tag_id = $1 AND implied_id <> $2 \
             UNION SELECT tag_id, $2 FROM tag_implications WHERE implied_id = $1 AND tag_id <> $2 \
             ON CONFLICT DO NOTHING",
        )
        .bind(old_id)
        .bind(target_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM tags WHERE id = $1")
            .bind(old_id)
            .execute(&mut *tx)
            .await?;
        apply_implications(&mut tx, target_id).await?;
    }
    sqlx::query(
        "INSERT INTO tag_aliases (alias, tag_id) VALUES ($1, $2) \
         ON CONFLICT (alias) DO UPDATE SET tag_id = EXCLUDED.tag_id",
    )
    .bind(&alias)
    .bind(target_id)
    .execute(&mut *tx)
    .await?;
    recount(&mut tx, &[target_id]).await?;
    tx.commit().await?;
    let tag = find(&state.db, &target.1).await?;
    Ok(Json(detail_of(&state.db, tag).await?))
}

/// `DELETE /tags/{name}/aliases/{alias}` (admin).
pub async fn remove_alias(
    State(state): State<AppState>,
    user: AuthUser,
    Path((name, alias)): Path<(String, String)>,
) -> AppResult<Json<TagDetail>> {
    require_admin(&user)?;
    let tag = find(&state.db, &name).await?;
    let alias = normalize_tag(&alias)?;
    let removed = sqlx::query("DELETE FROM tag_aliases WHERE alias = $1 AND tag_id = $2")
        .bind(&alias)
        .bind(tag.0)
        .execute(&state.db)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(detail_of(&state.db, tag).await?))
}

/// `POST /tags/{name}/implications {implies}` (admin). Existing posts with
/// the tag get the implied tags too.
pub async fn add_implication(
    State(state): State<AppState>,
    user: AuthUser,
    Path(name): Path<String>,
    Json(req): Json<ImplicationRequest>,
) -> AppResult<Json<TagDetail>> {
    require_admin(&user)?;
    let mut tx = state.db.begin().await?;
    lock_tag_graph(&mut tx).await?;
    let tag = find_or_create(&mut tx, &name).await?;
    let implied = find_or_create(&mut tx, &req.implies).await?;
    if tag.0 == implied.0 {
        return Err(AppError::BadRequest("a tag can't imply itself".into()));
    }
    if closure(&mut tx, implied.0).await?.contains(&tag.0) {
        return Err(AppError::Conflict {
            code: "implication_cycle",
            message: format!("`{}` already implies `{}`", implied.1, tag.1),
        });
    }
    sqlx::query(
        "INSERT INTO tag_implications (tag_id, implied_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(tag.0)
    .bind(implied.0)
    .execute(&mut *tx)
    .await?;
    apply_implications(&mut tx, tag.0).await?;
    tx.commit().await?;
    let tag = find(&state.db, &tag.1).await?;
    Ok(Json(detail_of(&state.db, tag).await?))
}

/// `DELETE /tags/{name}/implications/{implied}` (admin). Posts keep the tags
/// they already have.
pub async fn remove_implication(
    State(state): State<AppState>,
    user: AuthUser,
    Path((name, implied)): Path<(String, String)>,
) -> AppResult<Json<TagDetail>> {
    require_admin(&user)?;
    let tag = find(&state.db, &name).await?;
    let implied = find(&state.db, &implied).await?;
    let removed = sqlx::query("DELETE FROM tag_implications WHERE tag_id = $1 AND implied_id = $2")
        .bind(tag.0)
        .bind(implied.0)
        .execute(&state.db)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(detail_of(&state.db, tag).await?))
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    use super::like_prefix;
    use crate::testing;

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(like_prefix("blue_sk"), "blue\\_sk%");
        assert_eq!(like_prefix("100%"), "100\\%%");
        assert_eq!(like_prefix("a\\b"), "a\\\\b%");
    }

    async fn tags_of(app: &testing::TestApp, t: &str, id: i64) -> serde_json::Value {
        app.get(&format!("/api/v1/posts/{id}"), t).await.json()["tags"].clone()
    }

    #[tokio::test]
    async fn aliases_merge_and_resolve() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let user = app.register("user").await;
        let a = app.upload_png(&admin, 1, "kitty").await;
        let b = app.upload_png(&admin, 2, "cat kitty").await;

        let r = app
            .call(
                Method::POST,
                "/api/v1/tags/cat/aliases",
                Some(&user),
                Some(json!({"alias": "kitty"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        let r = app
            .call(
                Method::POST,
                "/api/v1/tags/cat/aliases",
                Some(&admin),
                Some(json!({"alias": "Kitty"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["aliases"], json!(["kitty"]));
        assert_eq!(r.json()["tag"]["post_count"], 2);

        // Posts moved over, without duplicates.
        assert_eq!(tags_of(&app, &admin, a).await, json!(["cat"]));
        assert_eq!(tags_of(&app, &admin, b).await, json!(["cat"]));
        // New tagging and searching go through the alias.
        let c = app.upload_png(&user, 3, "kitty").await;
        assert_eq!(tags_of(&app, &user, c).await, json!(["cat"]));
        let found = app.get("/api/v1/posts?q=kitty", &user).await.json();
        assert_eq!(found["posts"].as_array().unwrap().len(), 3);
        // Autocomplete finds the tag by its alias; the detail page resolves it.
        let auto = app.get("/api/v1/tags?q=kit", &user).await.json();
        assert_eq!(auto[0]["name"], "cat");
        let detail = app.get("/api/v1/tags/kitty", &user).await.json();
        assert_eq!(detail["tag"]["name"], "cat");

        let r = app
            .call(
                Method::DELETE,
                "/api/v1/tags/cat/aliases/kitty",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.json()["aliases"], json!([]));
        let d = app.upload_png(&user, 4, "kitty").await;
        assert_eq!(tags_of(&app, &user, d).await, json!(["kitty"]));
        app.finish().await;
    }

    #[tokio::test]
    async fn implications_apply_transitively() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let old = app.upload_png(&admin, 1, "kitten").await;
        let imply = |from: &'static str, to: &'static str| {
            let app = &app;
            let admin = admin.clone();
            async move {
                app.call(
                    Method::POST,
                    &format!("/api/v1/tags/{from}/implications"),
                    Some(&admin),
                    Some(json!({"implies": to})),
                )
                .await
            }
        };
        assert_eq!(imply("kitten", "cat").await.status, StatusCode::OK);
        assert_eq!(imply("cat", "animal").await.status, StatusCode::OK);
        // Existing posts were updated, through the chain.
        assert_eq!(
            tags_of(&app, &admin, old).await,
            json!(["animal", "cat", "kitten"])
        );
        // New posts too.
        let new = app.upload_png(&admin, 2, "kitten").await;
        assert_eq!(
            tags_of(&app, &admin, new).await,
            json!(["animal", "cat", "kitten"])
        );
        let animal = app.get("/api/v1/tags/animal", &admin).await.json();
        assert_eq!(animal["tag"]["post_count"], 2);
        assert_eq!(animal["implied_by"][0]["name"], "cat");

        // Cycles are refused.
        let r = imply("animal", "kitten").await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["code"], "implication_cycle");
        assert_eq!(imply("cat", "cat").await.status, StatusCode::BAD_REQUEST);

        let r = app
            .call(
                Method::DELETE,
                "/api/v1/tags/cat/implications/animal",
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.json()["implies"], json!([]));
        let newer = app.upload_png(&admin, 3, "kitten").await;
        assert_eq!(tags_of(&app, &admin, newer).await, json!(["cat", "kitten"]));
        app.finish().await;
    }

    #[tokio::test]
    async fn tag_list_pages_and_orders() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        app.upload_png(&t, 1, "b_tag a_tag").await;
        app.upload_png(&t, 2, "b_tag").await;
        let names = |v: serde_json::Value| -> Vec<String> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(
            names(app.get("/api/v1/tags", &t).await.json()),
            ["b_tag", "a_tag"]
        );
        assert_eq!(
            names(app.get("/api/v1/tags?order=name", &t).await.json()),
            ["a_tag", "b_tag"]
        );
        assert_eq!(
            names(
                app.get("/api/v1/tags?order=name&limit=1&page=2", &t)
                    .await
                    .json()
            ),
            ["b_tag"]
        );
        let r = app
            .call(
                Method::PATCH,
                "/api/v1/tags/a_tag",
                Some(&t),
                Some(json!({"category": "artist"})),
            )
            .await;
        assert_eq!(r.json()["category"], "artist");
        assert_eq!(
            app.get("/api/v1/tags/nope", &t).await.status,
            StatusCode::NOT_FOUND
        );
        app.finish().await;
    }
}
