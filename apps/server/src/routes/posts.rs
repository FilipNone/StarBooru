use axum::Json;
use axum::extract::multipart::MultipartError;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{FromRow, Postgres, QueryBuilder, Transaction};
use starbooru_core::query::{SearchQuery, SortKey};
use starbooru_core::tag::normalize_tags;
use starbooru_core::{
    DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT, MediaKind, Neighbors, Post, PostPage, PostUpdate, Rating,
    VoteRequest, parse_query, parse_tag_list,
};
use tracing::warn;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::{Ingested, Storage, Variant};

const MAX_PAGE: u32 = 10_000;
const MAX_SOURCE_CHARS: usize = 2048;

#[derive(FromRow)]
struct PostRow {
    id: i64,
    sha256: Vec<u8>,
    ext: String,
    mime: String,
    kind: i16,
    file_size: i64,
    width: Option<i32>,
    height: Option<i32>,
    has_thumb: bool,
    rating: i16,
    source: Option<String>,
    score: i32,
    created_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
    tags: Vec<String>,
    my_vote: i16,
}

/// `SELECT <post columns> FROM posts p` with the caller's vote resolved.
fn select_posts(user_id: i64) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(
        "SELECT p.id, p.sha256, p.ext, p.mime, p.kind, p.file_size, p.width, p.height, \
         p.has_thumb, p.rating, p.source, p.score, p.created_at, p.deleted_at, \
         COALESCE((SELECT array_agg(t.name ORDER BY t.name) FROM tags t \
                   WHERE t.id = ANY(p.tag_ids)), '{}'::text[]) AS tags, \
         COALESCE((SELECT v.value FROM post_votes v \
                   WHERE v.post_id = p.id AND v.user_id = ",
    );
    qb.push_bind(user_id);
    qb.push("), 0::smallint) AS my_vote FROM posts p");
    qb
}

fn to_post(state: &AppState, row: PostRow) -> Post {
    let sha = hex::encode(&row.sha256);
    let now = Utc::now().timestamp();
    let file_url = state.signer.url(
        Variant::Original,
        &Storage::file_name(Variant::Original, &sha, &row.ext),
        now,
    );
    let thumb_url = row.has_thumb.then(|| {
        state.signer.url(
            Variant::Thumb,
            &Storage::file_name(Variant::Thumb, &sha, &row.ext),
            now,
        )
    });
    Post {
        id: row.id,
        kind: MediaKind::from_db(row.kind).unwrap_or(MediaKind::Image),
        mime: row.mime,
        file_size: row.file_size,
        width: row.width,
        height: row.height,
        sha256: sha,
        rating: Rating::from_db(row.rating).unwrap_or_default(),
        score: row.score,
        my_vote: row.my_vote,
        source: row.source,
        tags: row.tags,
        created_at: row.created_at,
        deleted: row.deleted_at.is_some(),
        file_url,
        thumb_url,
    }
}

async fn load_row(state: &AppState, user_id: i64, id: i64) -> AppResult<PostRow> {
    let mut qb = select_posts(user_id);
    qb.push(" WHERE p.id = ").push_bind(id);
    qb.build_query_as::<PostRow>()
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

async fn load(state: &AppState, user_id: i64, id: i64) -> AppResult<Post> {
    Ok(to_post(state, load_row(state, user_id, id).await?))
}

/// Ids of the tags `names` refer to (aliases resolved), and how many distinct
/// tags were asked for.
async fn tag_ids(state: &AppState, names: &[String]) -> AppResult<(Vec<i64>, usize)> {
    let names = super::tags::canonical_names(&state.db, names).await?;
    if names.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let ids = sqlx::query_scalar("SELECT id FROM tags WHERE name = ANY($1)")
        .bind(&names)
        .fetch_all(&state.db)
        .await?;
    Ok((ids, names.len()))
}

/// Sort expression for a key; the id is always the tie-breaker. These match
/// the partial indexes in migration 0002.
fn sort_column(key: SortKey) -> Option<&'static str> {
    match key {
        SortKey::Id | SortKey::Random => None,
        SortKey::Score => Some("p.score"),
        SortKey::Size => Some("p.file_size"),
        SortKey::Pixels => Some("(p.width::bigint * p.height)"),
        SortKey::TagCount => Some("cardinality(p.tag_ids)"),
        SortKey::Updated => Some("p.updated_at"),
    }
}

/// ` WHERE ...` for a parsed search, given its resolved tag ids.
fn push_filters(
    qb: &mut QueryBuilder<Postgres>,
    query: &SearchQuery,
    include: Vec<i64>,
    exclude: Vec<i64>,
) {
    qb.push(if query.deleted {
        " WHERE p.deleted_at IS NOT NULL"
    } else {
        " WHERE p.deleted_at IS NULL"
    });
    if !include.is_empty() {
        qb.push(" AND p.tag_ids @> ").push_bind(include);
    }
    if !exclude.is_empty() {
        qb.push(" AND NOT (p.tag_ids && ")
            .push_bind(exclude)
            .push(")");
    }
    if !query.ratings.is_empty() {
        let r: Vec<i16> = query.ratings.iter().map(|r| r.to_db()).collect();
        qb.push(" AND p.rating = ANY(").push_bind(r).push(")");
    }
    if !query.exclude_ratings.is_empty() {
        let r: Vec<i16> = query.exclude_ratings.iter().map(|r| r.to_db()).collect();
        qb.push(" AND NOT (p.rating = ANY(").push_bind(r).push("))");
    }
    if let Some(kind) = query.kind {
        qb.push(" AND p.kind = ").push_bind(kind.to_db());
    }
}

#[derive(Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    q: String,
    page: Option<u32>,
    limit: Option<u32>,
}

/// `GET /posts?q=&page=&limit=`. Query language: `starbooru_core::query`.
pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<SearchParams>,
) -> AppResult<Json<PostPage>> {
    let page = params.page.unwrap_or(1).clamp(1, MAX_PAGE);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let query = parse_query(&params.q)?;
    let empty = || {
        Json(PostPage {
            posts: Vec::new(),
            page,
            limit,
            has_more: false,
        })
    };

    let (include, wanted) = tag_ids(&state, &query.include).await?;
    if include.len() < wanted {
        return Ok(empty()); // a required tag does not exist
    }
    let (exclude, _) = tag_ids(&state, &query.exclude).await?;

    let mut qb = select_posts(user.id);
    push_filters(&mut qb, &query, include, exclude);
    if query.order.key == SortKey::Random {
        qb.push(" ORDER BY random()");
    } else {
        let dir = if query.order.descending {
            " DESC"
        } else {
            " ASC"
        };
        qb.push(" ORDER BY ");
        if let Some(column) = sort_column(query.order.key) {
            qb.push(column).push(dir);
            if query.order.key == SortKey::Pixels {
                qb.push(" NULLS LAST");
            }
            qb.push(", ");
        }
        qb.push("p.id").push(dir);
    }
    let offset = i64::from(page - 1) * i64::from(limit);
    qb.push(" LIMIT ")
        .push_bind(i64::from(limit) + 1)
        .push(" OFFSET ")
        .push_bind(offset);

    let mut rows = qb.build_query_as::<PostRow>().fetch_all(&state.db).await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    Ok(Json(PostPage {
        posts: rows.into_iter().map(|r| to_post(&state, r)).collect(),
        page,
        limit,
        has_more,
    }))
}

#[derive(Deserialize)]
pub struct NeighborParams {
    #[serde(default)]
    q: String,
}

/// The sort expression used to step from one post to the next. Unlike
/// `sort_column` it is never NULL, so row comparisons work: posts without
/// dimensions sort last either way, as in `search`.
fn step_column(key: SortKey, descending: bool) -> Option<&'static str> {
    match (key, descending) {
        (SortKey::Pixels, true) => Some("COALESCE(p.width::bigint * p.height, -1)"),
        (SortKey::Pixels, false) => {
            Some("COALESCE(p.width::bigint * p.height, 9223372036854775807)")
        }
        _ => sort_column(key),
    }
}

/// `GET /posts/{id}/neighbors?q=`: the posts just before and after `id` in
/// the results of search `q`, for previous/next in the viewer. A random
/// order has no neighbours.
pub async fn neighbors(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
    Query(params): Query<NeighborParams>,
) -> AppResult<Json<Neighbors>> {
    let query = parse_query(&params.q)?;
    let none = Neighbors {
        prev: None,
        next: None,
    };
    if query.order.key == SortKey::Random {
        return Ok(Json(none));
    }
    let (include, wanted) = tag_ids(&state, &query.include).await?;
    if include.len() < wanted {
        return Ok(Json(none));
    }
    let (exclude, _) = tag_ids(&state, &query.exclude).await?;
    let column = step_column(query.order.key, query.order.descending);

    let step = |forward: bool| {
        // Forward follows the gallery order; backward reverses it.
        let ascending = forward != query.order.descending;
        let (op, dir) = if ascending {
            (" > ", " ASC")
        } else {
            (" < ", " DESC")
        };
        let mut qb = QueryBuilder::<Postgres>::new("SELECT p.id FROM posts p");
        push_filters(&mut qb, &query, include.clone(), exclude.clone());
        qb.push(" AND p.id <> ").push_bind(id).push(" AND ");
        match column {
            Some(c) => {
                qb.push("(")
                    .push(c)
                    .push(", p.id)")
                    .push(op)
                    .push("(SELECT ")
                    .push(c)
                    .push(", p.id FROM posts p WHERE p.id = ")
                    .push_bind(id)
                    .push(") ORDER BY ")
                    .push(c)
                    .push(dir)
                    .push(", p.id")
                    .push(dir);
            }
            None => {
                qb.push("p.id")
                    .push(op)
                    .push_bind(id)
                    .push(" ORDER BY p.id")
                    .push(dir);
            }
        }
        qb.push(" LIMIT 1");
        qb
    };
    let mut next = step(true);
    let mut prev = step(false);
    Ok(Json(Neighbors {
        prev: prev.build_query_scalar().fetch_optional(&state.db).await?,
        next: next.build_query_scalar().fetch_optional(&state.db).await?,
    }))
}

pub async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Post>> {
    Ok(Json(load(&state, user.id, id).await?))
}

fn multipart_error(e: MultipartError) -> AppError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::PayloadTooLarge
    } else {
        AppError::BadRequest(e.body_text())
    }
}

/// `POST /posts` (multipart): `file`, optional `tags`, `rating`, `source`.
pub async fn upload(
    State(state): State<AppState>,
    user: AuthUser,
    mut form: Multipart,
) -> AppResult<Json<Post>> {
    let mut file = None;
    let mut tags = Vec::new();
    let mut rating = Rating::default();
    let mut source = None;
    while let Some(field) = form.next_field().await.map_err(multipart_error)? {
        match field.name().unwrap_or_default() {
            "file" => file = Some(field.bytes().await.map_err(multipart_error)?),
            "tags" => tags = parse_tag_list(&field.text().await.map_err(multipart_error)?)?,
            "rating" => {
                let text = field.text().await.map_err(multipart_error)?;
                rating = Rating::parse(text.trim())
                    .ok_or_else(|| AppError::BadRequest(format!("unknown rating `{text}`")))?;
            }
            "source" => source = clean_source(field.text().await.map_err(multipart_error)?)?,
            _ => {} // ignore unknown fields for forward compatibility
        }
    }
    let bytes = file.ok_or_else(|| AppError::BadRequest("missing `file` field".into()))?;
    let file_size = i64::try_from(bytes.len()).map_err(AppError::internal)?;

    let worker = state.clone();
    let info = tokio::task::spawn_blocking(move || worker.storage.ingest(&bytes)).await??;

    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM posts WHERE sha256 = $1")
        .bind(info.sha256.as_slice())
        .fetch_optional(&state.db)
        .await?;
    if let Some(id) = existing {
        return Err(duplicate(id));
    }

    match insert_post(&state, &user, &info, file_size, rating, source, &tags).await {
        Ok(id) => Ok(Json(load(&state, user.id, id).await?)),
        Err(e) => {
            discard_orphan(&state, &info).await;
            Err(e)
        }
    }
}

/// After a failed insert, deletes the stored files unless a post owns them.
async fn discard_orphan(state: &AppState, info: &Ingested) {
    let owned: Result<bool, _> =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM posts WHERE sha256 = $1)")
            .bind(info.sha256.as_slice())
            .fetch_one(&state.db)
            .await;
    if let Ok(false) = owned
        && let Err(e) = state.storage.remove(&hex::encode(info.sha256), info.ext)
    {
        warn!("cannot remove orphan upload files: {e}");
    }
}

async fn insert_post(
    state: &AppState,
    user: &AuthUser,
    info: &Ingested,
    file_size: i64,
    rating: Rating,
    source: Option<String>,
    tags: &[String],
) -> AppResult<i64> {
    let mut tx = state.db.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO posts (sha256, ext, mime, kind, file_size, width, height, has_thumb, \
         rating, source, uploader_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
    )
    .bind(info.sha256.as_slice())
    .bind(info.ext)
    .bind(info.mime)
    .bind(info.kind.to_db())
    .bind(file_size)
    .bind(info.width)
    .bind(info.height)
    .bind(info.has_thumb)
    .bind(rating.to_db())
    .bind(source)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => duplicate(0),
        other => other,
    })?;
    set_tags(&mut tx, id, tags).await?;
    tx.commit().await?;
    Ok(id)
}

fn duplicate(id: i64) -> AppError {
    AppError::Conflict {
        code: "duplicate",
        message: if id > 0 {
            format!("this file is already post #{id}")
        } else {
            "this file was already uploaded".into()
        },
    }
}

fn clean_source(raw: String) -> AppResult<Option<String>> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if s.chars().count() > MAX_SOURCE_CHARS {
        return Err(AppError::BadRequest("source is too long".into()));
    }
    Ok(Some(s.to_owned()))
}

/// Replaces a live post's tags (aliases resolved, implications added),
/// creating missing tags and keeping `tags.post_count` in step.
async fn set_tags(
    tx: &mut Transaction<'_, Postgres>,
    post_id: i64,
    names: &[String],
) -> AppResult<()> {
    let new_ids = super::tags::resolve_for_post(tx, names).await?;
    let old_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT tag_ids FROM posts WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(post_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(AppError::NotFound)?;

    let added: Vec<i64> = new_ids
        .iter()
        .filter(|id| !old_ids.contains(id))
        .copied()
        .collect();
    let removed: Vec<i64> = old_ids
        .iter()
        .filter(|id| !new_ids.contains(id))
        .copied()
        .collect();
    sqlx::query("UPDATE posts SET tag_ids = $2, updated_at = now() WHERE id = $1")
        .bind(post_id)
        .bind(&new_ids)
        .execute(&mut **tx)
        .await?;
    adjust_counts(tx, &added, 1).await?;
    adjust_counts(tx, &removed, -1).await
}

async fn adjust_counts(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[i64],
    delta: i32,
) -> AppResult<()> {
    if !ids.is_empty() {
        sqlx::query("UPDATE tags SET post_count = GREATEST(post_count + $2, 0) WHERE id = ANY($1)")
            .bind(ids)
            .bind(delta)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(update): Json<PostUpdate>,
) -> AppResult<Json<Post>> {
    let mut tx = state.db.begin().await?;
    let live: Option<bool> =
        sqlx::query_scalar("SELECT deleted_at IS NULL FROM posts WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    match live {
        None => return Err(AppError::NotFound),
        Some(false) => {
            return Err(AppError::Conflict {
                code: "deleted",
                message: "restore the post before editing it".into(),
            });
        }
        Some(true) => {}
    }
    if let Some(tags) = &update.tags {
        let tags = normalize_tags(tags.iter().map(String::as_str))?;
        set_tags(&mut tx, id, &tags).await?;
    }
    if let Some(rating) = update.rating {
        sqlx::query("UPDATE posts SET rating = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(rating.to_db())
            .execute(&mut *tx)
            .await?;
    }
    if let Some(source) = update.source {
        sqlx::query("UPDATE posts SET source = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(clean_source(source)?)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(load(&state, user.id, id).await?))
}

/// Soft delete: moves the post to the recycle bin.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    set_deleted(&state, &user, id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn restore(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Post>> {
    set_deleted(&state, &user, id, false).await?;
    Ok(Json(load(&state, user.id, id).await?))
}

async fn set_deleted(state: &AppState, user: &AuthUser, id: i64, delete: bool) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    let (uploader, deleted, tag_ids): (Option<i64>, bool, Vec<i64>) = sqlx::query_as(
        "SELECT uploader_id, deleted_at IS NOT NULL, tag_ids FROM posts WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if !user.can_moderate(uploader) {
        return Err(AppError::Forbidden);
    }
    if deleted == delete {
        return Ok(()); // already in the requested state
    }
    sqlx::query(
        "UPDATE posts SET deleted_at = CASE WHEN $2 THEN now() ELSE NULL END, \
         updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(delete)
    .execute(&mut *tx)
    .await?;
    adjust_counts(&mut tx, &tag_ids, if delete { -1 } else { 1 }).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn vote(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<VoteRequest>,
) -> AppResult<Json<Post>> {
    if !(-1..=1).contains(&req.value) {
        return Err(AppError::BadRequest("vote must be -1, 0 or 1".into()));
    }
    let mut tx = state.db.begin().await?;
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM posts WHERE id = $1 AND deleted_at IS NULL FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    exists.ok_or(AppError::NotFound)?;
    let old: i16 =
        sqlx::query_scalar("SELECT value FROM post_votes WHERE post_id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(0);
    if req.value == 0 {
        sqlx::query("DELETE FROM post_votes WHERE post_id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO post_votes (post_id, user_id, value) VALUES ($1, $2, $3) \
             ON CONFLICT (post_id, user_id) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(id)
        .bind(user.id)
        .bind(req.value)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE posts SET score = score + $2 WHERE id = $1")
        .bind(id)
        .bind(i32::from(req.value) - i32::from(old))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(load(&state, user.id, id).await?))
}

/// `POST /posts/{id}/purge`: permanently deletes a post that is in the recycle bin.
pub async fn purge(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let row: (Option<i64>, bool) =
        sqlx::query_as("SELECT uploader_id, deleted_at IS NOT NULL FROM posts WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
    if !user.can_moderate(row.0) {
        return Err(AppError::Forbidden);
    }
    if !row.1 {
        return Err(AppError::Conflict {
            code: "not_deleted",
            message: "move the post to the recycle bin first".into(),
        });
    }
    purge_posts(&state, Purge::One(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Which recycle-bin posts to purge.
pub enum Purge {
    One(i64),
    OlderThanDays(u32),
}

/// Deletes recycle-bin posts and their files. Tag counts were already
/// lowered when the posts were soft-deleted.
pub async fn purge_posts(state: &AppState, which: Purge) -> AppResult<u64> {
    let query = match which {
        Purge::One(id) => sqlx::query_as(
            "DELETE FROM posts WHERE deleted_at IS NOT NULL AND id = $1 RETURNING sha256, ext",
        )
        .bind(id),
        Purge::OlderThanDays(days) => sqlx::query_as(
            "DELETE FROM posts WHERE deleted_at IS NOT NULL \
             AND deleted_at <= now() - make_interval(days => $1) RETURNING sha256, ext",
        )
        .bind(i32::try_from(days).unwrap_or(i32::MAX)),
    };
    let rows: Vec<(Vec<u8>, String)> = query.fetch_all(&state.db).await?;
    let count = rows.len() as u64;
    let worker = state.clone();
    tokio::task::spawn_blocking(move || {
        for (sha, ext) in rows {
            if let Err(e) = worker.storage.remove(&hex::encode(sha), &ext) {
                warn!("purged post left files behind: {e}");
            }
        }
    })
    .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use crate::testing;

    async fn order_of(app: &testing::TestApp, token: &str, q: &str) -> Vec<i64> {
        let page = app.get(&format!("/api/v1/posts?q={q}"), token).await.json();
        page["posts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_i64().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn sorts_by_every_key() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        // Distinct sizes, dimensions and tag counts.
        let small = app
            .upload(&t, &testing::png(4, 4, 1), &[("tags", "a b c")])
            .await
            .json()["id"]
            .as_i64()
            .unwrap();
        let big = app
            .upload(&t, &testing::png(64, 64, 2), &[("tags", "a")])
            .await
            .json()["id"]
            .as_i64()
            .unwrap();
        let mid = app
            .upload(&t, &testing::png(16, 16, 3), &[("tags", "a b")])
            .await
            .json()["id"]
            .as_i64()
            .unwrap();

        assert_eq!(order_of(&app, &t, "").await, vec![mid, big, small]);
        assert_eq!(order_of(&app, &t, "order:old").await, vec![small, big, mid]);
        assert_eq!(
            order_of(&app, &t, "order:size").await,
            vec![big, mid, small]
        );
        assert_eq!(
            order_of(&app, &t, "order:size_asc").await,
            vec![small, mid, big]
        );
        assert_eq!(
            order_of(&app, &t, "order:pixels").await,
            vec![big, mid, small]
        );
        assert_eq!(
            order_of(&app, &t, "order:tags").await,
            vec![small, mid, big]
        );
        assert_eq!(
            order_of(&app, &t, "order:tags_asc").await,
            vec![big, mid, small]
        );
        app.call(
            axum::http::Method::PATCH,
            &format!("/api/v1/posts/{small}"),
            Some(&t),
            Some(serde_json::json!({"rating": "sensitive"})),
        )
        .await;
        assert_eq!(
            order_of(&app, &t, "order:updated").await.first(),
            Some(&small)
        );
        let mut random = order_of(&app, &t, "order:random").await;
        random.sort_unstable();
        assert_eq!(random.len(), 3);
        let r = app.get("/api/v1/posts?q=order:sideways", &t).await;
        assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST);
        app.finish().await;
    }

    #[tokio::test]
    async fn neighbors_follow_the_search_order() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        // Ties on score and tag count, so the id tie-breaker matters too.
        for (i, (size, tags)) in [(4, "a b"), (64, "a"), (16, "a b"), (32, "a c"), (8, "b")]
            .into_iter()
            .enumerate()
        {
            let png = testing::png(size, size, u32::try_from(i).unwrap());
            app.upload(&t, &png, &[("tags", tags)]).await;
        }
        let step = |id: i64, q: &'static str| {
            let app = &app;
            let t = t.clone();
            async move {
                let path = format!("/api/v1/posts/{id}/neighbors?q={q}");
                let n = app.get(&path, &t).await.json();
                (n["prev"].as_i64(), n["next"].as_i64())
            }
        };
        for q in [
            "",
            "order:old",
            "order:score",
            "order:size",
            "order:size_asc",
            "order:pixels",
            "order:pixels_asc",
            "order:tags",
            "order:tags_asc",
            "order:updated_asc",
            "a",
            "a+-c+order:size",
        ] {
            let expected = order_of(&app, &t, q).await;
            assert!(!expected.is_empty(), "{q}");
            let mut walked = vec![expected[0]];
            let mut prev_of_first = None;
            loop {
                let current = *walked.last().unwrap();
                let (prev, next) = step(current, q).await;
                if walked.len() == 1 {
                    prev_of_first = prev;
                } else {
                    assert_eq!(
                        prev,
                        Some(walked[walked.len() - 2]),
                        "{q}: prev of {current}"
                    );
                }
                match next {
                    Some(n) => walked.push(n),
                    None => break,
                }
            }
            assert_eq!(prev_of_first, None, "{q}");
            assert_eq!(walked, expected, "{q}");
        }
        assert_eq!(step(1, "order:random").await, (None, None));
        assert_eq!(step(1, "missing_tag").await, (None, None));
        app.finish().await;
    }
}
