use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::multipart::MultipartError;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{FromRow, Postgres, QueryBuilder, Transaction};
use starbooru_core::query::{SearchQuery, SortKey};
use starbooru_core::tag::normalize_tags;
use starbooru_core::{
    BulkEdit, BulkResult, DEFAULT_PAGE_LIMIT, DEFAULT_RELATED_LIMIT, ImportRequest, MAX_BULK_POSTS,
    MAX_PAGE_LIMIT, MAX_RELATED_LIMIT, MediaKind, Neighbors, Post, PostPage, PostUpdate, Rating,
    RelatedPosts, VoteRequest, parse_query, parse_tag_list,
};
use tracing::warn;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::fetch::FetchError;
use crate::similar;
use crate::state::AppState;
use crate::storage::{Ingested, Storage, Variant};

const MAX_PAGE: u32 = 10_000;
const MAX_SOURCE_CHARS: usize = 2048;

#[derive(FromRow)]
pub(super) struct PostRow {
    id: i64,
    sha256: Vec<u8>,
    ext: String,
    mime: String,
    kind: i16,
    file_size: i64,
    width: Option<i32>,
    height: Option<i32>,
    duration_ms: Option<i64>,
    has_thumb: bool,
    has_sample: Option<bool>,
    rating: i16,
    source: Option<String>,
    score: i32,
    created_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
    tags: Vec<String>,
    my_vote: i16,
}

/// `SELECT <post columns>` with the caller's vote resolved, for `posts p`.
fn post_columns(user_id: i64) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(
        "SELECT p.id, p.sha256, p.ext, p.mime, p.kind, p.file_size, p.width, p.height, \
         p.duration_ms, p.has_thumb, p.has_sample, p.rating, p.source, p.score, p.created_at, p.deleted_at, \
         COALESCE((SELECT array_agg(t.name ORDER BY t.name) FROM tags t \
                   WHERE t.id = ANY(p.tag_ids)), '{}'::text[]) AS tags, \
         COALESCE((SELECT v.value FROM post_votes v \
                   WHERE v.post_id = p.id AND v.user_id = ",
    );
    qb.push_bind(user_id);
    qb.push("), 0::smallint) AS my_vote");
    qb
}

/// `SELECT <post columns> FROM posts p` with the caller's vote resolved.
pub(super) fn select_posts(user_id: i64) -> QueryBuilder<Postgres> {
    let mut qb = post_columns(user_id);
    qb.push(" FROM posts p");
    qb
}

/// Starts a query for a page of posts in two steps: the caller pushes the
/// rest of `SELECT p.id FROM posts p …` (filters, order, limit, offset),
/// then `end_page` turns those ids into posts in the same order. Only the
/// rows on the page are built, not every row an `OFFSET` skips or a sort
/// looks at, which keeps deep pages and big sorts fast.
pub(super) fn select_page(user_id: i64) -> QueryBuilder<Postgres> {
    let mut qb = post_columns(user_id);
    qb.push(" FROM unnest(ARRAY(SELECT p.id FROM posts p");
    qb
}

pub(super) fn end_page(qb: &mut QueryBuilder<Postgres>) {
    qb.push(")) WITH ORDINALITY AS page(id, n) JOIN posts p ON p.id = page.id ORDER BY page.n");
}

/// Signed URLs of a post's original and, if it has one, its thumbnail.
pub(super) fn media_urls(
    state: &AppState,
    sha: &str,
    ext: &str,
    has_thumb: bool,
) -> (String, Option<String>) {
    let now = Utc::now().timestamp();
    let file_url = state.signer.url(
        Variant::Original,
        &Storage::file_name(Variant::Original, sha, ext),
        now,
    );
    let thumb_url = has_thumb.then(|| {
        state.signer.url(
            Variant::Thumb,
            &Storage::file_name(Variant::Thumb, sha, ext),
            now,
        )
    });
    (file_url, thumb_url)
}

/// Signed URL of a post's sample, if it has one.
pub(super) fn sample_url(state: &AppState, sha: &str, has_sample: Option<bool>) -> Option<String> {
    (has_sample == Some(true)).then(|| {
        state.signer.url(
            Variant::Sample,
            &Storage::file_name(Variant::Sample, sha, ""),
            Utc::now().timestamp(),
        )
    })
}

pub(super) fn to_post(state: &AppState, row: PostRow) -> Post {
    let sha = hex::encode(&row.sha256);
    let (file_url, thumb_url) = media_urls(state, &sha, &row.ext, row.has_thumb);
    let sample_url = sample_url(state, &sha, row.has_sample);
    Post {
        id: row.id,
        kind: MediaKind::from_db(row.kind).unwrap_or(MediaKind::Image),
        mime: row.mime,
        file_size: row.file_size,
        width: row.width,
        height: row.height,
        duration_ms: row.duration_ms,
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
        sample_url,
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

pub(super) async fn load(state: &AppState, user_id: i64, id: i64) -> AppResult<Post> {
    Ok(to_post(state, load_row(state, user_id, id).await?))
}

/// Ids of the tags `names` refer to (aliases resolved), and how many distinct
/// tags were asked for.
pub(super) async fn tag_ids(state: &AppState, names: &[String]) -> AppResult<(Vec<i64>, usize)> {
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
pub(super) fn push_filters(
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

    let mut qb = select_page(user.id);
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
    end_page(&mut qb);

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

#[derive(Deserialize)]
pub struct RelatedParams {
    limit: Option<u32>,
}

/// Related posts are found through the post's rarest tags: this many, each
/// giving up to `RELATED_PER_TAG` of its newest posts.
const RELATED_SEED_TAGS: usize = 8;
const RELATED_PER_TAG: i64 = 200;
/// Look-alikes found per probed hash chunk value, at most.
const RELATED_PER_CHUNK: i64 = 25;
/// Hamming distance below which a hash adds to the score, and the worth of
/// each bit under it: an identical hash adds 9, about what a tag on one
/// post in 10,000 is worth.
const RELATED_LOOK_BITS: i64 = 12;
const RELATED_LOOK_WEIGHT: f64 = 0.75;
/// The planner's row estimate stands in for an exact count from this size.
const ESTIMATE_FROM: f32 = 10_000.0;

/// How much sharing a tag on `count` of `total` posts says: its inverse
/// document frequency. A tag on three posts says a lot, one on half the
/// collection almost nothing, one on every post nothing.
fn tag_weight(total: f64, count: i32) -> f64 {
    ((total + 1.0) / (f64::from(count.max(0)) + 1.0))
        .ln()
        .max(0.0)
}

/// `GET /posts/{id}/related?limit=`: live posts like `id`, most alike
/// first. Shared tags count by rarity and close hashes count too, so
/// look-alikes show up even without tags.
///
/// Only cheap lookups find candidates: the newest posts of each of the
/// rarest tags (the GIN index), and posts whose hash has a 16-bit chunk
/// within one bit of the post's (the chunk indexes of migration 0015, which
/// catch every hash within 7 bits). The queries are one-shot, so each is
/// planned for its actual tags: a rare tag through the GIN index, a common
/// one by walking the newest posts.
pub async fn related(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(params): Query<RelatedParams>,
) -> AppResult<Json<RelatedPosts>> {
    let limit = params
        .limit
        .unwrap_or(DEFAULT_RELATED_LIMIT)
        .clamp(1, MAX_RELATED_LIMIT);
    let (tags, hash): (Vec<i64>, Option<i64>) =
        sqlx::query_as("SELECT tag_ids, phash FROM posts WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;

    // Tags no other post has can't relate it to anything.
    let mut counts: Vec<(i64, i32)> =
        sqlx::query_as("SELECT id, post_count FROM tags WHERE id = ANY($1) AND post_count > 1")
            .bind(&tags)
            .fetch_all(&state.db)
            .await?;
    let total = if counts.is_empty() {
        0.0
    } else {
        live_posts(&state).await?
    };
    counts.sort_by_key(|&(id, count)| (count, id));
    let weighted: Vec<(i64, f64)> = counts
        .iter()
        .map(|&(id, count)| (id, tag_weight(total, count)))
        .filter(|&(_, w)| w > 0.0)
        .collect();
    let reach = hash
        .map(|h| similar::chunk_reach(u64::from_be_bytes(h.to_be_bytes()), 1))
        .unwrap_or_default();
    if weighted.is_empty() && reach.is_empty() {
        return Ok(Json(RelatedPosts::default()));
    }

    let (ids, weights): (Vec<i64>, Vec<f64>) = weighted.iter().copied().unzip();
    let mut qb = QueryBuilder::<Postgres>::new("WITH w(id, w) AS (SELECT * FROM unnest(");
    qb.push_bind(ids)
        .push("::bigint[], ")
        .push_bind(weights)
        .push("::float8[])), cand AS (SELECT NULL::bigint AS id");
    for &(tag, _) in weighted.iter().take(RELATED_SEED_TAGS) {
        qb.push(
            " UNION (SELECT p.id FROM posts p WHERE p.deleted_at IS NULL AND p.tag_ids @> ARRAY[",
        )
        .push_bind(tag)
        .push("]::bigint[] ORDER BY p.id DESC LIMIT ")
        .push_bind(RELATED_PER_TAG)
        .push(")");
    }
    for (chunk, values) in reach.into_iter().enumerate() {
        let column = match chunk {
            0 => "(p.phash & 65535)",
            1 => "((p.phash >> 16) & 65535)",
            2 => "((p.phash >> 32) & 65535)",
            _ => "((p.phash >> 48) & 65535)",
        };
        qb.push(" UNION (SELECT l.id FROM unnest(")
            .push_bind(values)
            .push("::bigint[]) v CROSS JOIN LATERAL (SELECT p.id FROM posts p WHERE p.deleted_at IS NULL AND ")
            .push(column)
            .push(" = v LIMIT ")
            .push_bind(RELATED_PER_CHUNK)
            .push(") l)");
    }
    qb.push(
        ") SELECT p.id FROM cand JOIN posts p ON p.id = cand.id \
         CROSS JOIN LATERAL (SELECT \
             COALESCE((SELECT sum(w.w) FROM w WHERE w.id = ANY(p.tag_ids)), 0) \
             + COALESCE(GREATEST(",
    )
    .push_bind(RELATED_LOOK_BITS)
    .push(" - bit_count((p.phash # ")
    .push_bind(hash)
    .push("::bigint)::bit(64)), 0) * ")
    .push_bind(RELATED_LOOK_WEIGHT)
    .push(", 0) AS score) s WHERE p.id <> ")
    .push_bind(id)
    .push(" AND s.score > 0 ORDER BY s.score DESC, p.id DESC LIMIT ")
    .push_bind(i64::from(limit));
    let ranked: Vec<i64> = qb
        .build_query_scalar()
        .persistent(false)
        .fetch_all(&state.db)
        .await?;

    let mut qb = select_posts(user.id);
    qb.push(" WHERE p.id = ANY(").push_bind(&ranked).push(")");
    let mut rows: HashMap<i64, PostRow> = qb
        .build_query_as::<PostRow>()
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let posts = ranked
        .iter()
        .filter_map(|id| rows.remove(id))
        .map(|r| to_post(&state, r))
        .collect();
    Ok(Json(RelatedPosts { posts }))
}

/// About how many live posts there are: exact for a small collection, the
/// planner's estimate (`reltuples`, -1 before the first `ANALYZE`) for a
/// big one, where counting would read the whole table.
async fn live_posts(state: &AppState) -> AppResult<f64> {
    let (estimate,): (f32,) =
        sqlx::query_as("SELECT reltuples FROM pg_class WHERE oid = 'posts'::regclass")
            .fetch_one(&state.db)
            .await?;
    if estimate >= ESTIMATE_FROM {
        return Ok(f64::from(estimate));
    }
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM posts WHERE deleted_at IS NULL")
        .fetch_one(&state.db)
        .await?;
    // Exact: below `ESTIMATE_FROM` posts, or barely past it before an `ANALYZE`.
    Ok(f64::from(i32::try_from(count).unwrap_or(i32::MAX)))
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
    create_post(&state, &user, bytes, rating, source, &tags)
        .await
        .map(Json)
}

/// `POST /posts/import`: downloads a URL and adds it like an upload.
pub async fn import(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ImportRequest>,
) -> AppResult<Json<Post>> {
    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("URL import is turned off on this server".into()))?;
    let tags = normalize_tags(req.tags.iter().map(String::as_str))?;
    let source = clean_source(req.source.unwrap_or_else(|| req.url.clone()))?;
    let fetched = fetcher
        .get(&req.url, state.config.max_upload_bytes)
        .await
        .map_err(|e| match e {
            FetchError::BadUrl | FetchError::Blocked => AppError::BadRequest(e.to_string()),
            FetchError::TooLarge => AppError::PayloadTooLarge,
            _ => AppError::Upstream(e.to_string()),
        })?;
    create_post(
        &state,
        &user,
        Bytes::from(fetched),
        req.rating,
        source,
        &tags,
    )
    .await
    .map(Json)
}

/// Stores `bytes` and inserts the post, refusing exact duplicates.
pub(super) async fn create_post(
    state: &AppState,
    user: &AuthUser,
    bytes: Bytes,
    rating: Rating,
    source: Option<String>,
    tags: &[String],
) -> AppResult<Post> {
    let file_size = i64::try_from(bytes.len()).map_err(AppError::internal)?;
    // Early, so a file over the limit isn't processed; checked again below.
    check_quota(&state.db, user.id, file_size).await?;

    let worker = state.clone();
    let info = tokio::task::spawn_blocking(move || worker.storage.ingest(&bytes)).await??;

    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM posts WHERE sha256 = $1")
        .bind(info.sha256.as_slice())
        .fetch_optional(&state.db)
        .await?;
    if let Some(id) = existing {
        return Err(duplicate(id));
    }

    match insert_post(state, user, &info, file_size, rating, source, tags).await {
        Ok(id) => load(state, user.id, id).await,
        Err(e) => {
            discard_orphan(state, &info).await;
            Err(e)
        }
    }
}

/// Refuses a file that would take the uploader past their storage limit.
/// Locks the account row, so one account's uploads can't race past it.
async fn check_quota(db: impl sqlx::PgExecutor<'_>, user_id: i64, adding: i64) -> AppResult<()> {
    let (quota, used): (Option<i64>, i64) = sqlx::query_as(
        "SELECT quota_bytes, CASE WHEN quota_bytes IS NULL THEN 0 ELSE \
             (SELECT coalesce(sum(file_size), 0)::bigint FROM posts WHERE uploader_id = $1) END \
         FROM users WHERE id = $1 FOR UPDATE",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    match quota {
        Some(q) if used.saturating_add(adding) > q => Err(AppError::QuotaExceeded),
        _ => Ok(()),
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
    let auto_tags = crate::settings::current(state).await?.auto_meta_tags;
    let mut tx = state.db.begin().await?;
    check_quota(&mut *tx, user.id, file_size).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO posts (sha256, ext, mime, kind, file_size, width, height, duration_ms, \
         has_thumb, rating, source, uploader_id, phash, pcolor, has_sample) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING id",
    )
    .bind(info.sha256.as_slice())
    .bind(info.ext)
    .bind(info.mime)
    .bind(info.kind.to_db())
    .bind(file_size)
    .bind(info.width)
    .bind(info.height)
    .bind(info.duration_ms)
    .bind(info.has_thumb)
    .bind(rating.to_db())
    .bind(source)
    .bind(user.id)
    .bind(info.print.map(|p| p.hash_db()))
    .bind(info.print.map(|p| p.color_db()))
    .bind(info.has_sample)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => duplicate(0),
        other => other,
    })?;
    set_tags(&mut tx, id, tags).await?;
    if auto_tags {
        let facts = super::autotag::facts(
            info.kind,
            info.width,
            info.height,
            info.duration_ms,
            info.has_audio,
        );
        super::autotag::tag_post(&mut tx, id, &facts).await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// id, sha256, ext, kind.
type BackfillRow = (i64, Vec<u8>, String, i16);

/// At startup: thumbnails, dimensions and duration for posts that have no
/// thumbnail yet and that ffmpeg can read (uploaded before it was installed,
/// or before this existed). Files ffmpeg can't read are retried next start.
pub async fn backfill_media(state: AppState) {
    if !state.storage.has_tools() {
        return;
    }
    let mut after = 0i64;
    let mut done = 0u64;
    loop {
        let batch: Result<Vec<BackfillRow>, _> = sqlx::query_as(
            "SELECT id, sha256, ext, kind FROM posts \
             WHERE NOT has_thumb AND (kind = $1 OR ext IN ('avif', 'jxl')) AND id > $2 \
             ORDER BY id LIMIT 50",
        )
        .bind(MediaKind::Video.to_db())
        .bind(after)
        .fetch_all(&state.db)
        .await;
        let batch = match batch {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => break,
            Err(e) => {
                warn!("media backfill stopped: {e}");
                return;
            }
        };
        for (id, sha, ext, kind) in batch {
            after = id;
            let kind = MediaKind::from_db(kind).unwrap_or(MediaKind::Image);
            let sha_hex = hex::encode(sha);
            let st = state.clone();
            let derived =
                tokio::task::spawn_blocking(move || st.storage.derive(&sha_hex, &ext, kind)).await;
            let derived = match derived {
                Ok(Ok(d)) => d,
                Ok(Err(e)) => {
                    warn!("media backfill: post {id}: {e}");
                    continue;
                }
                Err(e) => {
                    warn!("media backfill: post {id}: {e}");
                    continue;
                }
            };
            if !derived.has_thumb && derived.width.is_none() && derived.duration_ms.is_none() {
                continue;
            }
            let updated = sqlx::query(
                "UPDATE posts SET has_thumb = has_thumb OR $2, width = COALESCE(width, $3), \
                 height = COALESCE(height, $4), duration_ms = COALESCE(duration_ms, $5), \
                 phash = COALESCE(phash, $6), pcolor = COALESCE(pcolor, $7) WHERE id = $1",
            )
            .bind(id)
            .bind(derived.has_thumb)
            .bind(derived.width)
            .bind(derived.height)
            .bind(derived.duration_ms)
            .bind(derived.print.map(|p| p.hash_db()))
            .bind(derived.print.map(|p| p.color_db()))
            .execute(&state.db)
            .await;
            match updated {
                Ok(_) => done += 1,
                Err(e) => warn!("media backfill: post {id}: {e}"),
            }
        }
    }
    if done > 0 {
        tracing::info!("media backfill: updated {done} posts");
    }
}

/// At startup: samples for big images not looked at yet (uploaded before
/// samples existed, or while they were off).
pub async fn backfill_samples(state: AppState) {
    if !state.storage.makes_samples() {
        return;
    }
    let mut after = 0i64;
    let mut made = 0u64;
    loop {
        let batch: Result<Vec<(i64, Vec<u8>, String)>, _> = sqlx::query_as(
            "SELECT id, sha256, ext FROM posts WHERE has_sample IS NULL AND id > $1 \
             ORDER BY id LIMIT 50",
        )
        .bind(after)
        .fetch_all(&state.db)
        .await;
        let batch = match batch {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => break,
            Err(e) => {
                warn!("sample backfill stopped: {e}");
                return;
            }
        };
        for (id, sha, ext) in batch {
            after = id;
            let sha_hex = hex::encode(sha);
            let st = state.clone();
            let made_one =
                tokio::task::spawn_blocking(move || st.storage.make_sample_for(&sha_hex, &ext))
                    .await;
            let has_sample = match made_one {
                Ok(Ok(made_one)) => made_one,
                Ok(Err(e)) => {
                    warn!("sample backfill: post {id}: {e}");
                    continue;
                }
                Err(e) => {
                    warn!("sample backfill: post {id}: {e}");
                    continue;
                }
            };
            let updated = sqlx::query("UPDATE posts SET has_sample = $2 WHERE id = $1")
                .bind(id)
                .bind(has_sample)
                .execute(&state.db)
                .await;
            match updated {
                Ok(_) => made += u64::from(has_sample),
                Err(e) => warn!("sample backfill: post {id}: {e}"),
            }
        }
    }
    if made > 0 {
        tracing::info!("sample backfill: made {made} samples");
    }
}

/// Fingerprints posts that have a thumbnail but no hash yet (uploaded
/// before the duplicate finder existed). Runs once at startup.
pub async fn backfill_hashes(state: AppState) {
    let mut after = 0i64;
    let mut done = 0u64;
    loop {
        let batch: Result<Vec<(i64, Vec<u8>, String)>, _> = sqlx::query_as(
            "SELECT id, sha256, ext FROM posts \
             WHERE (phash IS NULL OR pcolor IS NULL) AND has_thumb AND id > $1 \
             ORDER BY id LIMIT 200",
        )
        .bind(after)
        .fetch_all(&state.db)
        .await;
        let batch = match batch {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => break,
            Err(e) => {
                warn!("hash backfill stopped: {e}");
                return;
            }
        };
        let Some(&(last, ..)) = batch.last() else {
            break;
        };
        after = last;
        let st = state.clone();
        let hashed = tokio::task::spawn_blocking(move || {
            batch
                .into_iter()
                .filter_map(|(id, sha, ext)| {
                    st.storage
                        .thumb_print(&hex::encode(sha), &ext)
                        .map(|p| (id, p.hash_db(), p.color_db()))
                })
                .collect::<Vec<(i64, i64, i32)>>()
        })
        .await;
        let Ok(hashed) = hashed else {
            warn!("hash backfill stopped: worker failed");
            return;
        };
        let ids: Vec<i64> = hashed.iter().map(|h| h.0).collect();
        let hashes: Vec<i64> = hashed.iter().map(|h| h.1).collect();
        let colors: Vec<i32> = hashed.iter().map(|h| h.2).collect();
        let updated = sqlx::query(
            "UPDATE posts p SET phash = h.phash, pcolor = h.pcolor \
             FROM UNNEST($1::bigint[], $2::bigint[], $3::int[]) AS h(id, phash, pcolor) \
             WHERE p.id = h.id",
        )
        .bind(&ids)
        .bind(&hashes)
        .bind(&colors)
        .execute(&state.db)
        .await;
        match updated {
            Ok(r) => done += r.rows_affected(),
            Err(e) => {
                warn!("hash backfill stopped: {e}");
                return;
            }
        }
    }
    if done > 0 {
        tracing::info!("hash backfill: fingerprinted {done} posts");
    }
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
pub(super) async fn set_tags(
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

pub(super) async fn adjust_counts(
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

/// `POST /posts/bulk`: adds and removes tags and sets the rating on many
/// posts in one transaction, or moves them to the recycle bin.
pub async fn bulk(
    State(state): State<AppState>,
    user: AuthUser,
    Json(edit): Json<BulkEdit>,
) -> AppResult<Json<BulkResult>> {
    let mut ids = edit.ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() || ids.len() > MAX_BULK_POSTS {
        return Err(AppError::BadRequest(format!(
            "select between 1 and {MAX_BULK_POSTS} posts"
        )));
    }
    let edits = !edit.add_tags.is_empty() || !edit.remove_tags.is_empty() || edit.rating.is_some();
    if edit.delete {
        if edits {
            return Err(AppError::BadRequest(
                "delete posts or edit them, not both at once".into(),
            ));
        }
        let mut result = BulkResult::default();
        for id in ids {
            match set_deleted(&state, &user, id, true).await {
                Ok(()) => result.updated += 1,
                Err(AppError::NotFound | AppError::Forbidden | AppError::Conflict { .. }) => {
                    result.skipped += 1;
                }
                Err(e) => return Err(e),
            }
        }
        return Ok(Json(result));
    }
    if !edits {
        return Err(AppError::BadRequest("nothing to change".into()));
    }
    let add = normalize_tags(edit.add_tags.iter().map(String::as_str))?;
    let remove = normalize_tags(edit.remove_tags.iter().map(String::as_str))?;

    let mut tx = state.db.begin().await?;
    // Removing an alias removes its tag.
    let remove = super::tags::canonical_names(&mut *tx, &remove).await?;
    // Locked in id order, so concurrent bulk edits can't deadlock.
    let live: Vec<(i64, Vec<String>)> = sqlx::query_as(
        "SELECT p.id, ARRAY(SELECT t.name FROM tags t WHERE t.id = ANY(p.tag_ids) ORDER BY t.name) \
         FROM posts p WHERE p.id = ANY($1) AND p.deleted_at IS NULL ORDER BY p.id FOR UPDATE",
    )
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await?;
    if !add.is_empty() || !remove.is_empty() {
        for (id, current) in &live {
            let mut names: Vec<String> = current
                .iter()
                .filter(|t| !remove.contains(t))
                .cloned()
                .collect();
            names.extend(
                add.iter()
                    .filter(|t| !names.contains(t))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            if names.len() > starbooru_core::tag::MAX_TAGS_PER_POST {
                return Err(starbooru_core::tag::TagError::TooMany.into());
            }
            set_tags(&mut tx, *id, &names).await?;
        }
    }
    let live_ids: Vec<i64> = live.iter().map(|(id, _)| *id).collect();
    if let Some(rating) = edit.rating {
        sqlx::query("UPDATE posts SET rating = $2, updated_at = now() WHERE id = ANY($1)")
            .bind(&live_ids)
            .bind(rating.to_db())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let updated = live_ids.len() as u64;
    Ok(Json(BulkResult {
        updated,
        skipped: (ids.len() as u64).saturating_sub(updated),
    }))
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

pub(super) async fn set_deleted(
    state: &AppState,
    user: &AuthUser,
    id: i64,
    delete: bool,
) -> AppResult<()> {
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
             ON CONFLICT (post_id, user_id) DO UPDATE SET value = EXCLUDED.value, \
             voted_at = CASE WHEN post_votes.value = EXCLUDED.value \
                             THEN post_votes.voted_at ELSE now() END",
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
    use axum::http::{Method, StatusCode};

    use super::tag_weight;

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

    #[test]
    fn rare_tags_weigh_more() {
        let w = |count| tag_weight(1_000_000.0, count);
        assert!(w(3) > w(1_000) && w(1_000) > w(500_000));
        assert!((w(3) - (1_000_001f64 / 4.0).ln()).abs() < 1e-9);
        // On every post, or counted on more than the estimate: worthless, never negative.
        assert_eq!(w(1_000_000), 0.0);
        assert_eq!(tag_weight(10.0, 50), 0.0);
        assert_eq!(tag_weight(0.0, -3), 0.0);
    }

    #[tokio::test]
    async fn related_ranks_rare_tags_and_look_alikes_first() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let mut ids = Vec::new();
        for (seed, tags) in [
            "common rare_x mid_a", // the post
            "common rare_x mid_a", // a: shares everything
            "common mid_a",        // b
            "common",              // c
            "other",               // d: nothing in common
            "rare_x",              // e: deleted below
            "",                    // f: no tags, but looks alike
            "common mid_a",        // g
            "common",              // h
        ]
        .into_iter()
        .enumerate()
        {
            ids.push(
                app.upload_png(&t, 100 + u32::try_from(seed).unwrap(), tags)
                    .await,
            );
        }
        let [post, a, b, c, _d, e, f, g, h]: [i64; 9] = ids.try_into().unwrap();
        let hash: i64 = 0x0f0f_3c3c_5a5a_9696;
        sqlx::query("UPDATE posts SET phash = $1")
            .bind(!hash)
            .execute(app.db())
            .await
            .unwrap();
        for (id, value) in [(post, hash), (f, hash ^ 0b101)] {
            sqlx::query("UPDATE posts SET phash = $2 WHERE id = $1")
                .bind(id)
                .bind(value)
                .execute(app.db())
                .await
                .unwrap();
        }
        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/posts/{e}"),
                Some(&t),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);

        let related = |path: String| {
            let app = &app;
            let t = t.clone();
            async move {
                let r = app.get(&path, &t).await;
                assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
                r.json()["posts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["id"].as_i64().unwrap())
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(
            related(format!("/api/v1/posts/{post}/related")).await,
            [f, a, g, b, h, c]
        );
        assert_eq!(
            related(format!("/api/v1/posts/{post}/related?limit=2")).await,
            [f, a]
        );
        // Without a hash only the tags count.
        sqlx::query("UPDATE posts SET phash = NULL WHERE id = $1")
            .bind(post)
            .execute(app.db())
            .await
            .unwrap();
        assert_eq!(
            related(format!("/api/v1/posts/{post}/related")).await,
            [a, g, b, h, c]
        );
        let r = app.get("/api/v1/posts/999999/related", &t).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        app.finish().await;
    }

    #[tokio::test]
    async fn backfill_adds_video_thumbnails() {
        let Some(tools) = crate::probe::tests::tools() else {
            return;
        };
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let src = tempfile::tempdir().unwrap();
        let Some(clip) = crate::probe::tests::generate(
            src.path(),
            "clip.mp4",
            "testsrc=duration=2:size=64x48:rate=10",
            &["-pix_fmt", "yuv420p"],
        ) else {
            return app.finish().await;
        };
        // The test app has no ffmpeg, like a server that predates it.
        let r = app.upload(&t, &clip, &[("tags", "clip")]).await.json();
        let id = r["id"].as_i64().unwrap();
        assert!(r["thumb_url"].is_null());
        assert!(r["duration_ms"].is_null());

        let mut config = app.state.config.clone();
        config.tools = Some(tools.clone());
        let storage = crate::storage::Storage::open(&config.data_dir)
            .await
            .unwrap()
            .with_tools(Some(tools));
        let with_ffmpeg = crate::state::AppState::new(config, app.state.db.clone(), storage);
        super::backfill_media(with_ffmpeg).await;

        let p = app.get(&format!("/api/v1/posts/{id}"), &t).await.json();
        assert!(p["thumb_url"].is_string());
        assert_eq!(p["duration_ms"], 2000);
        assert_eq!(
            (p["width"].as_i64(), p["height"].as_i64()),
            (Some(64), Some(48))
        );
        let thumb = app.get(p["thumb_url"].as_str().unwrap(), &t).await;
        assert_eq!(thumb.status, axum::http::StatusCode::OK);
        app.finish().await;
    }

    #[tokio::test]
    async fn big_images_get_samples() {
        let Some(app) = testing::app_with(|c| c.samples = false).await else {
            return;
        };
        let t = app.register("admin").await;
        let big = testing::noise_png(800, 600, 1);
        let r = app.upload(&t, &big, &[("tags", "noise")]).await.json();
        let late = r["id"].as_i64().unwrap();
        assert!(r["sample_url"].is_null());
        let small = app.upload_png(&t, 1, "small").await;

        // Turned on later: the backfill makes the missing ones.
        let mut config = app.state.config.clone();
        config.samples = true;
        let storage = crate::storage::Storage::open(&config.data_dir)
            .await
            .unwrap();
        let on = crate::state::AppState::new(config, app.state.db.clone(), storage);
        super::backfill_samples(on.clone()).await;
        let p = app.get(&format!("/api/v1/posts/{late}"), &t).await.json();
        let url = p["sample_url"].as_str().unwrap().to_owned();
        assert!(url.starts_with("/media/sample/"), "{url}");
        let sample = app.call(Method::GET, &url, None, None).await;
        assert_eq!(sample.status, StatusCode::OK);
        assert_eq!(sample.headers["content-type"], "image/jpeg");
        assert!(sample.body.len() < big.len());
        let p = app.get(&format!("/api/v1/posts/{small}"), &t).await.json();
        assert!(p["sample_url"].is_null());
        let pending: i64 =
            sqlx::query_scalar("SELECT count(*) FROM posts WHERE has_sample IS NULL")
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(pending, 0);

        // With samples on, an upload gets one at once, and so does an edit.
        let router = crate::routes::router(on);
        let req = |method: Method, path: String, body: Option<serde_json::Value>| {
            let mut b = axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {t}"));
            let body = match body {
                Some(v) => {
                    b = b.header("content-type", "application/json");
                    axum::body::Body::from(v.to_string())
                }
                None => axum::body::Body::empty(),
            };
            b.body(body).unwrap()
        };
        use tower::ServiceExt;
        let edited = router
            .clone()
            .oneshot(req(
                Method::POST,
                format!("/api/v1/posts/{late}/edit"),
                Some(serde_json::json!({ "rotate": 90 })),
            ))
            .await
            .unwrap();
        assert_eq!(edited.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(edited.into_body(), usize::MAX)
            .await
            .unwrap();
        let edited: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(edited["sample_url"].is_string(), "{edited}");
        app.finish().await;
    }

    /// Serves a PNG, a redirect to it, a redirect loop, a 404 and a big file.
    async fn remote_server() -> String {
        use axum::response::Redirect;
        use axum::routing::get;
        let png = testing::png(16, 12, 7);
        let app = axum::Router::new()
            .route("/a.png", get(move || async move { png.clone() }))
            .route("/hop", get(|| async { Redirect::temporary("/a.png") }))
            .route("/loop", get(|| async { Redirect::temporary("/loop") }))
            .route("/big", get(|| async { vec![0u8; 2 * 1024 * 1024] }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn imports_from_a_url() {
        let Some(app) = testing::app_with(|c| {
            c.url_import = Some(crate::fetch::Reach::Any);
            c.max_upload_bytes = 1024 * 1024;
        })
        .await
        else {
            return;
        };
        let t = app.register("admin").await;
        let base = remote_server().await;
        let import = |url: String, tags: &[&str]| serde_json::json!({ "url": url, "tags": tags, "rating": "sensitive" });

        let r = app
            .call(
                Method::POST,
                "/api/v1/posts/import",
                Some(&t),
                Some(import(format!("{base}/hop"), &["Blue Sky", "x"])),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let p = r.json();
        assert_eq!(p["width"], 16);
        assert_eq!(p["rating"], "sensitive");
        assert_eq!(p["source"], format!("{base}/hop"));
        assert_eq!(p["tags"], serde_json::json!(["blue_sky", "x"]));

        let again = app
            .call(
                Method::POST,
                "/api/v1/posts/import",
                Some(&t),
                Some(import(format!("{base}/a.png"), &[])),
            )
            .await;
        assert_eq!(again.status, StatusCode::CONFLICT);
        assert_eq!(again.json()["code"], "duplicate");

        for (path, status, code) in [
            ("/missing", StatusCode::BAD_GATEWAY, "fetch_failed"),
            ("/loop", StatusCode::BAD_GATEWAY, "fetch_failed"),
            ("/big", StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
        ] {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/posts/import",
                    Some(&t),
                    Some(import(format!("{base}{path}"), &[])),
                )
                .await;
            assert_eq!(
                (r.status, r.json()["code"].as_str()),
                (status, Some(code)),
                "{path}"
            );
        }
        let bad = app
            .call(
                Method::POST,
                "/api/v1/posts/import",
                Some(&t),
                Some(import("ftp://x/y".into(), &[])),
            )
            .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        let no_auth = app
            .call(
                Method::POST,
                "/api/v1/posts/import",
                None,
                Some(import(format!("{base}/a.png"), &[])),
            )
            .await;
        assert_eq!(no_auth.status, StatusCode::UNAUTHORIZED);
        app.finish().await;
    }

    #[tokio::test]
    async fn import_refuses_private_addresses_by_default() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let base = remote_server().await;
        assert_eq!(app.get("/api/v1/info", &t).await.json()["url_import"], true);
        for url in [
            format!("{base}/a.png"),
            base.replace("127.0.0.1", "localhost") + "/a.png",
        ] {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/posts/import",
                    Some(&t),
                    Some(serde_json::json!({ "url": url })),
                )
                .await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST, "{url}");
            assert!(
                r.json()["message"]
                    .as_str()
                    .unwrap()
                    .contains("private network")
            );
        }
        app.finish().await;

        let Some(off) = testing::app_with(|c| c.url_import = None).await else {
            return;
        };
        let t = off.register("admin").await;
        assert_eq!(
            off.get("/api/v1/info", &t).await.json()["url_import"],
            false
        );
        let r = off
            .call(
                Method::POST,
                "/api/v1/posts/import",
                Some(&t),
                Some(serde_json::json!({ "url": "https://example.com/a.png" })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        off.finish().await;
    }

    #[tokio::test]
    async fn bulk_edits_tags_rating_and_deletes() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let user = app.register("user").await;
        app.call(
            Method::POST,
            "/api/v1/tags/cat/aliases",
            Some(&admin),
            Some(serde_json::json!({ "alias": "kitty" })),
        )
        .await;
        app.call(
            Method::POST,
            "/api/v1/tags/cat/implications",
            Some(&admin),
            Some(serde_json::json!({ "implies": "animal" })),
        )
        .await;
        let a = app.upload_png(&admin, 1, "cat sky").await;
        let b = app.upload_png(&admin, 2, "sky old").await;
        let c = app.upload_png(&user, 3, "sky").await;
        let gone = app.upload_png(&admin, 4, "sky").await;
        app.call(
            Method::DELETE,
            &format!("/api/v1/posts/{gone}"),
            Some(&admin),
            None,
        )
        .await;

        let r = app
            .call(
                Method::POST,
                "/api/v1/posts/bulk",
                Some(&user),
                Some(serde_json::json!({
                    "ids": [a, b, c, gone, 999_999, a],
                    "add_tags": ["Kitty", "night"],
                    "remove_tags": ["old", "sky"],
                    "rating": "questionable",
                })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json(), serde_json::json!({ "updated": 3, "skipped": 2 }));
        for id in [a, b, c] {
            let p = app.get(&format!("/api/v1/posts/{id}"), &user).await.json();
            assert_eq!(
                p["tags"],
                serde_json::json!(["animal", "cat", "night"]),
                "{id}"
            );
            assert_eq!(p["rating"], "questionable");
        }
        // The deleted post is untouched, and counts follow.
        let p = app
            .get(&format!("/api/v1/posts/{gone}"), &admin)
            .await
            .json();
        assert_eq!(p["tags"], serde_json::json!(["sky"]));
        let sky = app.get("/api/v1/tags/sky", &user).await.json();
        assert_eq!(sky["tag"]["post_count"], 0);

        // Removing an alias removes its tag; implied tags come back while cat stays.
        let r = app
            .call(
                Method::POST,
                "/api/v1/posts/bulk",
                Some(&user),
                Some(serde_json::json!({ "ids": [a], "remove_tags": ["kitty", "animal"] })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let p = app.get(&format!("/api/v1/posts/{a}"), &user).await.json();
        assert_eq!(p["tags"], serde_json::json!(["night"]));

        // Deleting: only your own posts unless you're an admin.
        let r = app
            .call(
                Method::POST,
                "/api/v1/posts/bulk",
                Some(&user),
                Some(serde_json::json!({ "ids": [a, c], "delete": true })),
            )
            .await;
        assert_eq!(r.json(), serde_json::json!({ "updated": 1, "skipped": 1 }));
        let r = app
            .call(
                Method::POST,
                "/api/v1/posts/bulk",
                Some(&admin),
                Some(serde_json::json!({ "ids": [a, b], "delete": true })),
            )
            .await;
        assert_eq!(r.json(), serde_json::json!({ "updated": 2, "skipped": 0 }));

        for body in [
            serde_json::json!({ "ids": [] , "rating": "general" }),
            serde_json::json!({ "ids": [a] }),
            serde_json::json!({ "ids": [a], "delete": true, "rating": "general" }),
            serde_json::json!({ "ids": (1..=501).collect::<Vec<i64>>(), "rating": "general" }),
            serde_json::json!({ "ids": [a], "add_tags": ["-bad"] }),
        ] {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/posts/bulk",
                    Some(&admin),
                    Some(body.clone()),
                )
                .await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}");
        }
        app.finish().await;
    }
}
