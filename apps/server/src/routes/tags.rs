//! Tags: autocomplete and the tag list, categories, aliases, implications
//! and the tag tree (parents, and the starter vocabulary seeded into it).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::{PgExecutor, PgPool, Postgres, Transaction};
use starbooru_core::{
    AliasRequest, ImplicationRequest, PostTag, SECTIONS, Tag, TagCategory, TagCorrection,
    TagDetail, TagOrder, TagUpdate, VARIANTS, VOCAB, VOCAB_VERSION, closest_tag, normalize_tag,
    split_category,
};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MAX_TAG_LIMIT: u32 = 100;
const MAX_TAG_PAGE: u32 = 10_000;
/// Serialises alias and implication changes, which rewrite many posts.
const TAG_GRAPH_LOCK: i64 = 7276;
/// Parent chains are followed this far at most.
const MAX_DEPTH: i32 = 32;
/// Unknown names looked up for a correction in one request.
const MAX_CORRECTIONS: usize = 8;
/// Names checked in one `GET /tag-corrections`.
const MAX_CHECKED_NAMES: usize = 200;

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

/// Tags spelled like `name` with at least `min_posts` posts, and their counts:
/// those `pg_trgm` finds similar, plus names as long with the same first letter,
/// as short words share few trigrams (`cta` and `cat`).
async fn spelled_like(db: &PgPool, name: &str, min_posts: i32) -> AppResult<Vec<(String, i32)>> {
    let first: String = name.chars().take(1).collect();
    let length = i32::try_from(name.chars().count()).unwrap_or(i32::MAX);
    Ok(sqlx::query_as(
        "(SELECT name, post_count FROM tags WHERE name % $1 AND post_count >= $2 \
           ORDER BY similarity(name, $1) DESC, post_count DESC LIMIT 20) \
         UNION \
         (SELECT name, post_count FROM tags WHERE name LIKE $3 ESCAPE '\\' \
           AND char_length(name) BETWEEN $4 AND $5 AND post_count >= $2 \
           ORDER BY post_count DESC LIMIT 200)",
    )
    .bind(name)
    .bind(min_posts)
    .bind(like_prefix(&first))
    .bind(length.saturating_sub(1))
    .bind(length.saturating_add(1))
    .fetch_all(db)
    .await?)
}

/// For each of `names` that no tag or alias has, the tag it probably meant, if
/// any. `used_only` leaves out tags on no post, where a search would find nothing.
pub(super) async fn corrections(
    db: &PgPool,
    names: &[String],
    used_only: bool,
) -> AppResult<Vec<TagCorrection>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let unknown: Vec<String> = sqlx::query_scalar(
        "SELECT n.name FROM unnest($1::text[]) WITH ORDINALITY AS n(name, ord) \
         WHERE NOT EXISTS (SELECT 1 FROM tags t WHERE t.name = n.name) \
           AND NOT EXISTS (SELECT 1 FROM tag_aliases a WHERE a.alias = n.name) \
         ORDER BY n.ord",
    )
    .bind(names)
    .fetch_all(db)
    .await?;
    let mut out: Vec<TagCorrection> = Vec::new();
    for name in unknown.into_iter().take(MAX_CORRECTIONS) {
        if out.iter().any(|c| c.from == name) {
            continue;
        }
        let candidates = spelled_like(db, &name, i32::from(used_only)).await?;
        let closest = closest_tag(&name, candidates.iter().map(|(n, c)| (n.as_str(), *c)));
        if let Some(to) = closest.map(str::to_owned) {
            out.push(TagCorrection { from: name, to });
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct CorrectionQuery {
    #[serde(default)]
    q: String,
}

/// `GET /tag-corrections?q=a+b`: "did you mean" for the unknown names in a
/// list being typed. A name with a category prefix is a new tag on purpose.
pub async fn tag_corrections(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<CorrectionQuery>,
) -> AppResult<Json<Vec<TagCorrection>>> {
    let names: Vec<String> =
        q.q.split_whitespace()
            .take(MAX_CHECKED_NAMES)
            .filter_map(|name| normalize_tag(name).ok())
            .filter(|name| split_category(name).0.is_none())
            .collect();
    Ok(Json(corrections(&state.db, &names, false).await?))
}

/// Drops category prefixes (`artist:`), replaces aliases with their tag and removes duplicates,
/// keeping first-seen order.
pub async fn canonical_names(db: impl PgExecutor<'_>, names: &[String]) -> AppResult<Vec<String>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let names: Vec<&str> = names.iter().map(|n| split_category(n).1).collect();
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
/// tags created, implied tags added (transitively). A category prefix
/// (`artist:name`) sets the tag's category.
pub async fn resolve_for_post(
    tx: &mut Transaction<'_, Postgres>,
    names: &[String],
) -> AppResult<Vec<i64>> {
    let (prefixed, categories): (Vec<&str>, Vec<i16>) = names
        .iter()
        .filter_map(|n| match split_category(n) {
            (Some(category), name) => Some((name, category.to_db())),
            (None, _) => None,
        })
        .unzip();
    let names = canonical_names(&mut **tx, names).await?;
    if names.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query("INSERT INTO tags (name) SELECT unnest($1::text[]) ON CONFLICT (name) DO NOTHING")
        .bind(&names)
        .execute(&mut **tx)
        .await?;
    if !prefixed.is_empty() {
        sqlx::query(
            "UPDATE tags t SET category = c.category \
             FROM unnest($1::text[], $2::smallint[]) AS c(name, category) \
             WHERE t.category <> c.category AND (t.name = c.name \
                OR t.id = (SELECT a.tag_id FROM tag_aliases a WHERE a.alias = c.name))",
        )
        .bind(&prefixed)
        .bind(&categories)
        .execute(&mut **tx)
        .await?;
    }
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

/// Each of `names` (already normalised, without prefixes) with the id and
/// name of the tag it means, following aliases. Missing tags are created.
pub(super) async fn by_name(
    tx: &mut Transaction<'_, Postgres>,
    names: &[String],
) -> AppResult<HashMap<String, (i64, String)>> {
    if names.is_empty() {
        return Ok(HashMap::new());
    }
    let canonical: Vec<(String, String)> = sqlx::query_as(
        "SELECT n.name, COALESCE(t.name, n.name) FROM unnest($1::text[]) AS n(name) \
         LEFT JOIN tag_aliases a ON a.alias = n.name LEFT JOIN tags t ON t.id = a.tag_id",
    )
    .bind(names)
    .fetch_all(&mut **tx)
    .await?;
    let targets: Vec<&str> = canonical.iter().map(|(_, c)| c.as_str()).collect();
    sqlx::query("INSERT INTO tags (name) SELECT unnest($1::text[]) ON CONFLICT (name) DO NOTHING")
        .bind(&targets)
        .execute(&mut **tx)
        .await?;
    let ids: HashMap<String, i64> =
        sqlx::query_as::<_, (String, i64)>("SELECT name, id FROM tags WHERE name = ANY($1)")
            .bind(&targets)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .collect();
    Ok(canonical
        .into_iter()
        .filter_map(|(name, target)| Some((name, (*ids.get(&target)?, target))))
        .collect())
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
    let name = split_category(&name).1;
    sqlx::query_as(
        "SELECT t.id, t.name, t.category, t.post_count FROM tags t \
         WHERE t.name = $1 OR t.id = (SELECT tag_id FROM tag_aliases WHERE alias = $1) \
         ORDER BY t.name = $1 DESC LIMIT 1",
    )
    .bind(name)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

/// Finds or creates the tag `raw` names (following an alias).
async fn find_or_create(tx: &mut Transaction<'_, Postgres>, raw: &str) -> AppResult<TagRow> {
    let name = normalize_tag(raw)?;
    let name = split_category(&name).1.to_owned();
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
    let children: Vec<TagRow> = sqlx::query_as(
        "SELECT id, name, category, post_count FROM tags WHERE parent_id = $1 ORDER BY name",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(TagDetail {
        tag: to_tag(tag),
        aliases,
        implies: implies.into_iter().map(to_tag).collect(),
        implied_by: implied_by.into_iter().map(to_tag).collect(),
        ancestors: ancestors(db, id).await?.into_iter().map(to_tag).collect(),
        children: children.into_iter().map(to_tag).collect(),
    })
}

/// The parents of tag `id`, top first.
async fn ancestors(db: impl PgExecutor<'_>, id: i64) -> AppResult<Vec<TagRow>> {
    Ok(sqlx::query_as(
        "WITH RECURSIVE up(id, depth) AS ( \
             SELECT parent_id, 1 FROM tags WHERE id = $1 AND parent_id IS NOT NULL \
             UNION ALL SELECT t.parent_id, up.depth + 1 FROM tags t JOIN up ON t.id = up.id \
             WHERE t.parent_id IS NOT NULL AND up.depth < $2 \
         ) SELECT t.id, t.name, t.category, t.post_count FROM up JOIN tags t ON t.id = up.id \
         ORDER BY up.depth DESC",
    )
    .bind(id)
    .bind(MAX_DEPTH)
    .fetch_all(db)
    .await?)
}

/// Each of `ids` with the tags below it (itself included), for searching.
pub(super) async fn descendants(
    db: impl PgExecutor<'_>,
    ids: &[i64],
) -> AppResult<HashMap<i64, Vec<i64>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(i64, Vec<i64>)> = sqlx::query_as(
        "WITH RECURSIVE d(root, id, depth) AS ( \
             SELECT id, id, 0 FROM tags WHERE id = ANY($1) \
             UNION ALL SELECT d.root, t.id, d.depth + 1 FROM tags t JOIN d ON t.parent_id = d.id \
             WHERE d.depth < $2 \
         ) SELECT root, array_agg(DISTINCT id) FROM d GROUP BY root",
    )
    .bind(ids)
    .bind(MAX_DEPTH)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// A post's tags with their category and section (the top of their tree).
pub(super) async fn post_tags(db: &PgPool, post_id: i64) -> AppResult<Vec<PostTag>> {
    let rows: Vec<(String, i16, Option<String>, bool)> = sqlx::query_as(
        "WITH RECURSIVE up(tag_id, id, depth) AS ( \
             SELECT t.id, t.id, 0 FROM posts p JOIN tags t ON t.id = ANY(p.tag_ids) \
             WHERE p.id = $1 \
             UNION ALL SELECT up.tag_id, t.parent_id, up.depth + 1 FROM tags t \
             JOIN up ON t.id = up.id WHERE t.parent_id IS NOT NULL AND up.depth < $2 \
         ) SELECT t.name, t.category, \
             (SELECT r.name FROM up JOIN tags r ON r.id = up.id WHERE up.tag_id = t.id \
              ORDER BY up.depth DESC LIMIT 1), \
             EXISTS (SELECT 1 FROM tags c WHERE c.parent_id = t.id) \
         FROM up JOIN tags t ON t.id = up.tag_id WHERE up.depth = 0 ORDER BY t.name",
    )
    .bind(post_id)
    .bind(MAX_DEPTH)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(name, category, root, has_children)| PostTag {
            section: root.filter(|r| *r != name || has_children),
            name,
            category: TagCategory::from_db(category).unwrap_or_default(),
        })
        .collect())
}

/// Whether making `parent` the parent of `child` would close a loop:
/// `parents` maps tags to their parents.
fn makes_cycle(parents: &HashMap<i64, i64>, child: i64, parent: i64) -> bool {
    let mut at = parent;
    for _ in 0..MAX_DEPTH {
        if at == child {
            return true;
        }
        match parents.get(&at) {
            Some(&up) => at = up,
            None => return false,
        }
    }
    true // deeper than any chain may be
}

/// Sets or clears (`None`) a tag's parent, refusing loops.
async fn set_parent(
    tx: &mut Transaction<'_, Postgres>,
    tag: &TagRow,
    parent: Option<&TagRow>,
) -> AppResult<()> {
    if let Some(parent) = parent {
        let above = ancestors(&mut **tx, parent.0).await?;
        if parent.0 == tag.0 || above.iter().any(|a| a.0 == tag.0) {
            return Err(AppError::Conflict {
                code: "parent_cycle",
                message: format!("`{}` is already under `{}`", parent.1, tag.1),
            });
        }
    }
    sqlx::query("UPDATE tags SET parent_id = $2 WHERE id = $1")
        .bind(tag.0)
        .bind(parent.map(|p| p.0))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// At startup: adds the starter vocabulary (`starbooru_core::vocab`) once per
/// version. Existing tags keep their parent; links that would loop are skipped.
pub async fn seed_vocab(db: &PgPool) -> AppResult<bool> {
    let mut tx = db.begin().await?;
    lock_tag_graph(&mut tx).await?;
    let seeded: Option<i32> = sqlx::query_scalar("SELECT vocab_version FROM server_settings")
        .fetch_optional(&mut *tx)
        .await?;
    if seeded.unwrap_or(0) >= VOCAB_VERSION {
        return Ok(false);
    }
    let names: Vec<String> = SECTIONS
        .iter()
        .chain(VOCAB.iter().map(|(tag, _)| tag))
        .map(|n| (*n).to_owned())
        .collect();
    sqlx::query(
        "INSERT INTO tags (name) SELECT n FROM unnest($1::text[]) AS n \
         WHERE NOT EXISTS (SELECT 1 FROM tag_aliases a WHERE a.alias = n) \
         ON CONFLICT (name) DO NOTHING",
    )
    .bind(&names)
    .execute(&mut *tx)
    .await?;
    // name (or alias) -> id, and the links already there
    let ids: HashMap<String, i64> = sqlx::query_as(
        "SELECT n, COALESCE(t.id, a.tag_id) FROM unnest($1::text[]) AS n \
         LEFT JOIN tags t ON t.name = n LEFT JOIN tag_aliases a ON a.alias = n \
         WHERE COALESCE(t.id, a.tag_id) IS NOT NULL",
    )
    .bind(&names)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let mut parents: HashMap<i64, i64> =
        sqlx::query_as("SELECT id, parent_id FROM tags WHERE parent_id IS NOT NULL")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let (mut children, mut links) = (Vec::new(), Vec::new());
    for (tag, parent) in VOCAB {
        let (Some(&child), Some(&parent)) = (ids.get(*tag), ids.get(*parent)) else {
            continue;
        };
        if child == parent || parents.contains_key(&child) || makes_cycle(&parents, child, parent) {
            continue;
        }
        parents.insert(child, parent);
        children.push(child);
        links.push(parent);
    }
    sqlx::query(
        "UPDATE tags t SET parent_id = l.parent FROM unnest($1::bigint[], $2::bigint[]) \
         AS l(child, parent) WHERE t.id = l.child",
    )
    .bind(&children)
    .bind(&links)
    .execute(&mut *tx)
    .await?;
    for (alias, tag) in VARIANTS {
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tag_aliases WHERE alias = $1)")
                .bind(*alias)
                .fetch_one(&mut *tx)
                .await?;
        if taken {
            continue;
        }
        let target = find_or_create(&mut tx, tag).await?;
        // The other way round already: someone made the tag an alias of this name.
        if target.1 != *alias {
            alias_into(&mut tx, &target, alias).await?;
        }
    }
    sqlx::query(
        "INSERT INTO server_settings (id, vocab_version) VALUES (true, $1) \
         ON CONFLICT (id) DO UPDATE SET vocab_version = EXCLUDED.vocab_version",
    )
    .bind(VOCAB_VERSION)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
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

/// `PATCH /tags/{name} {category?, parent?}`, creating the tag if needed.
pub async fn update(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(name): Path<String>,
    Json(update): Json<TagUpdate>,
) -> AppResult<Json<Tag>> {
    let mut tx = state.db.begin().await?;
    lock_tag_graph(&mut tx).await?;
    let tag = find_or_create(&mut tx, &name).await?;
    if let Some(category) = update.category {
        sqlx::query("UPDATE tags SET category = $2 WHERE id = $1")
            .bind(tag.0)
            .bind(category.to_db())
            .execute(&mut *tx)
            .await?;
    }
    match update.parent.as_deref().map(str::trim) {
        None => {}
        Some("") => set_parent(&mut tx, &tag, None).await?,
        Some(parent) => {
            let parent = find_or_create(&mut tx, parent).await?;
            if parent.0 == tag.0 {
                return Err(AppError::BadRequest("a tag can't be its own parent".into()));
            }
            set_parent(&mut tx, &tag, Some(&parent)).await?;
        }
    }
    tx.commit().await?;
    Ok(Json(to_tag(find(&state.db, &tag.1).await?)))
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
    user.require_admin()?;
    let alias = normalize_tag(&req.alias)?;
    let mut tx = state.db.begin().await?;
    lock_tag_graph(&mut tx).await?;
    let target = find_or_create(&mut tx, &name).await?;
    if alias == target.1 {
        return Err(AppError::BadRequest(
            "a tag can't be an alias of itself".into(),
        ));
    }
    alias_into(&mut tx, &target, &alias).await?;
    tx.commit().await?;
    let tag = find(&state.db, &target.1).await?;
    Ok(Json(detail_of(&state.db, tag).await?))
}

/// Makes `alias` another name for `target`. A tag with that name is merged in:
/// its posts, aliases and implications move over.
async fn alias_into(
    tx: &mut Transaction<'_, Postgres>,
    target: &TagRow,
    alias: &str,
) -> AppResult<()> {
    let target_id = target.0;
    let old: Option<i64> = sqlx::query_scalar("SELECT id FROM tags WHERE name = $1")
        .bind(alias)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(old_id) = old {
        sqlx::query(
            "UPDATE posts SET tag_ids = ARRAY(SELECT DISTINCT unnest( \
                 array_replace(tag_ids, $1::bigint, $2::bigint))), updated_at = now() \
             WHERE tag_ids @> ARRAY[$1::bigint]",
        )
        .bind(old_id)
        .bind(target_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query("UPDATE tag_aliases SET tag_id = $2 WHERE tag_id = $1")
            .bind(old_id)
            .bind(target_id)
            .execute(&mut **tx)
            .await?;
        // Its children move under the target, which takes its place in the
        // tree when it has none of its own.
        sqlx::query("UPDATE tags SET parent_id = $2 WHERE parent_id = $1 AND id <> $2")
            .bind(old_id)
            .bind(target_id)
            .execute(&mut **tx)
            .await?;
        let old_parent: Option<TagRow> = sqlx::query_as(
            "SELECT p.id, p.name, p.category, p.post_count FROM tags o \
             JOIN tags p ON p.id = o.parent_id WHERE o.id = $1",
        )
        .bind(old_id)
        .fetch_optional(&mut **tx)
        .await?;
        let target_parent: Option<i64> =
            sqlx::query_scalar("SELECT parent_id FROM tags WHERE id = $1")
                .bind(target_id)
                .fetch_one(&mut **tx)
                .await?;
        if let Some(parent) = old_parent.filter(|p| p.0 != target_id)
            && target_parent.is_none_or(|p| p == old_id)
        {
            match set_parent(tx, target, Some(&parent)).await {
                Err(AppError::Conflict { .. }) => {} // it would loop: stays at the top
                other => other?,
            }
        }
        sqlx::query(
            "INSERT INTO tag_implications (tag_id, implied_id) \
             SELECT $2, implied_id FROM tag_implications WHERE tag_id = $1 AND implied_id <> $2 \
             UNION SELECT tag_id, $2 FROM tag_implications WHERE implied_id = $1 AND tag_id <> $2 \
             ON CONFLICT DO NOTHING",
        )
        .bind(old_id)
        .bind(target_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO figure_tags (figure_id, tag_id, slot, position) \
             SELECT figure_id, $2, slot, position FROM figure_tags WHERE tag_id = $1 \
             ON CONFLICT DO NOTHING",
        )
        .bind(old_id)
        .bind(target_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query("DELETE FROM tags WHERE id = $1")
            .bind(old_id)
            .execute(&mut **tx)
            .await?;
        apply_implications(tx, target_id).await?;
        recount(tx, &[target_id]).await?;
    }
    sqlx::query(
        "INSERT INTO tag_aliases (alias, tag_id) VALUES ($1, $2) \
         ON CONFLICT (alias) DO UPDATE SET tag_id = EXCLUDED.tag_id",
    )
    .bind(alias)
    .bind(target_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// `DELETE /tags/{name}/aliases/{alias}` (admin).
pub async fn remove_alias(
    State(state): State<AppState>,
    user: AuthUser,
    Path((name, alias)): Path<(String, String)>,
) -> AppResult<Json<TagDetail>> {
    user.require_admin()?;
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
    user.require_admin()?;
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
    user.require_admin()?;
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

    use super::{like_prefix, makes_cycle, seed_vocab};
    use crate::testing;

    #[test]
    fn loops_are_found() {
        use std::collections::HashMap;
        let parents: HashMap<i64, i64> = [(2, 1), (3, 2)].into_iter().collect();
        assert!(makes_cycle(&parents, 1, 3)); // 1 under 3, which is under 1
        assert!(makes_cycle(&parents, 4, 4));
        assert!(!makes_cycle(&parents, 4, 3));
        let ring: HashMap<i64, i64> = [(1, 2), (2, 1)].into_iter().collect();
        assert!(makes_cycle(&ring, 9, 1)); // a loop already there never ends the walk
    }

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

    async fn patch(
        app: &testing::TestApp,
        t: &str,
        tag: &str,
        body: serde_json::Value,
    ) -> testing::Resp {
        app.call(
            Method::PATCH,
            &format!("/api/v1/tags/{tag}"),
            Some(t),
            Some(body),
        )
        .await
    }

    async fn found(app: &testing::TestApp, t: &str, q: &str) -> Vec<i64> {
        let r = app.get(&format!("/api/v1/posts?q={q}"), t).await.json();
        let mut ids: Vec<i64> = r["posts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_i64().unwrap())
            .collect();
        ids.sort_unstable();
        ids
    }

    #[tokio::test]
    async fn parents_find_their_children() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let boots = app.upload_png(&t, 1, "boots").await;
        let sneakers = app.upload_png(&t, 2, "sneakers smile").await;
        let hat = app.upload_png(&t, 3, "hat").await;
        let r = patch(&app, &t, "footwear", json!({"parent": "clothing"})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(
            patch(&app, &t, "boots", json!({"parent": "footwear"}))
                .await
                .status,
            StatusCode::OK
        );
        assert_eq!(
            patch(&app, &t, "sneakers", json!({"parent": "footwear"}))
                .await
                .status,
            StatusCode::OK
        );
        assert_eq!(
            patch(&app, &t, "hat", json!({"parent": "clothing"}))
                .await
                .status,
            StatusCode::OK
        );

        // A parent finds everything below it, at any depth; excluding one excludes its tree.
        assert_eq!(found(&app, &t, "footwear").await, [boots, sneakers]);
        assert_eq!(found(&app, &t, "clothing").await, [boots, sneakers, hat]);
        assert_eq!(found(&app, &t, "footwear+smile").await, [sneakers]);
        assert_eq!(found(&app, &t, "clothing+-footwear").await, [hat]);
        assert_eq!(found(&app, &t, "boots").await, [boots]);

        let d = app.get("/api/v1/tags/boots", &t).await.json();
        assert_eq!(d["ancestors"][0]["name"], "clothing");
        assert_eq!(d["ancestors"][1]["name"], "footwear");
        let d = app.get("/api/v1/tags/footwear", &t).await.json();
        assert_eq!(d["children"][0]["name"], "boots");

        // Loops are refused.
        let r = patch(&app, &t, "clothing", json!({"parent": "boots"})).await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["code"], "parent_cycle");
        let r = patch(&app, &t, "clothing", json!({"parent": "clothing"})).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);

        // The viewer's sections: the top of the tree, or nothing for a lone tag.
        let p = app
            .get(&format!("/api/v1/posts/{sneakers}"), &t)
            .await
            .json();
        assert_eq!(
            p["tag_details"],
            json!([
                {"name": "smile", "category": "general", "section": null},
                {"name": "sneakers", "category": "general", "section": "clothing"},
            ])
        );
        // So do the other single-post answers; lists leave them out.
        let r = app
            .call(
                Method::PUT,
                &format!("/api/v1/posts/{sneakers}/vote"),
                Some(&t),
                Some(json!({"value": 1})),
            )
            .await;
        assert_eq!(r.json()["tag_details"][1]["section"], "clothing");
        let list = app.get("/api/v1/posts", &t).await.json();
        assert!(list["posts"][0].get("tag_details").is_none());

        // "" removes the parent.
        assert_eq!(
            patch(&app, &t, "hat", json!({"parent": ""})).await.status,
            StatusCode::OK
        );
        assert_eq!(found(&app, &t, "clothing").await, [boots, sneakers]);
        app.finish().await;
    }

    #[tokio::test]
    async fn category_prefixes_set_the_category() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let id = app
            .upload_png(&t, 1, "artist:Some_One char:miku plain")
            .await;
        assert_eq!(
            tags_of(&app, &t, id).await,
            json!(["miku", "plain", "some_one"])
        );
        let artist = app.get("/api/v1/tags/some_one", &t).await.json();
        assert_eq!(artist["tag"]["category"], "artist");
        let p = app.get(&format!("/api/v1/posts/{id}"), &t).await.json();
        assert_eq!(p["tag_details"][0]["category"], "character");
        // The prefix works when searching, and leaves a category alone when absent.
        assert_eq!(found(&app, &t, "artist:some_one").await, [id]);
        let again = app.upload_png(&t, 2, "some_one").await;
        assert_eq!(tags_of(&app, &t, again).await, json!(["some_one"]));
        let artist = app.get("/api/v1/tags/artist:some_one", &t).await.json();
        assert_eq!(artist["tag"]["category"], "artist");
        app.finish().await;
    }

    #[tokio::test]
    async fn merging_an_alias_moves_its_place_in_the_tree() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        app.upload_png(&t, 1, "blond_hair").await;
        patch(&app, &t, "blond_hair", json!({"parent": "hair_color"})).await;
        patch(
            &app,
            &t,
            "platinum_blond_hair",
            json!({"parent": "blond_hair"}),
        )
        .await;
        let r = app
            .call(
                Method::POST,
                "/api/v1/tags/blonde_hair/aliases",
                Some(&t),
                Some(json!({"alias": "blond_hair"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["ancestors"][0]["name"], "hair_color");
        assert_eq!(r.json()["children"][0]["name"], "platinum_blond_hair");
        app.finish().await;
    }

    #[tokio::test]
    async fn spelling_variants_become_aliases() {
        let Some(app) = testing::app().await else {
            return;
        };
        let admin = app.register("admin").await;
        let name = |d: &serde_json::Value| d["tag"]["name"].as_str().unwrap().to_owned();
        let add_alias = |tag: &'static str, alias: &'static str| {
            let (app, admin) = (&app, &admin);
            async move {
                let r = app
                    .call(
                        Method::POST,
                        &format!("/api/v1/tags/{tag}/aliases"),
                        Some(admin),
                        Some(json!({"alias": alias})),
                    )
                    .await;
                assert_eq!(r.status, StatusCode::OK);
            }
        };
        // Posts under a variant spelling, and with both spellings.
        let gray = app.upload_png(&admin, 1, "gray_hair").await;
        let both = app.upload_png(&admin, 2, "gray_hair grey_hair").await;
        // Someone's own choices win: an alias elsewhere, and a tag the other way round.
        add_alias("my_blonde", "blond_hair").await;
        add_alias("two_boys", "2boys").await;
        assert!(seed_vocab(app.db()).await.unwrap());

        // Merged: the posts move over, and the old name now leads to the tag.
        assert_eq!(tags_of(&app, &admin, gray).await, json!(["grey_hair"]));
        assert_eq!(tags_of(&app, &admin, both).await, json!(["grey_hair"]));
        let d = app.get("/api/v1/tags/gray_hair", &admin).await.json();
        assert_eq!(name(&d), "grey_hair");
        assert_eq!(d["tag"]["post_count"], 2);
        // Not on any post yet: still an alias, for the next upload and search.
        let one_girl = app.upload_png(&admin, 3, "one_girl").await;
        assert_eq!(tags_of(&app, &admin, one_girl).await, json!(["1girl"]));
        let found = app.get("/api/v1/posts?q=one_girl", &admin).await.json();
        assert_eq!(found["posts"][0]["id"], one_girl);
        assert!(found.get("corrected").is_none(), "{found}");

        let d = app.get("/api/v1/tags/blond_hair", &admin).await.json();
        assert_eq!(name(&d), "my_blonde");
        let d = app.get("/api/v1/tags/2boys", &admin).await.json();
        assert_eq!(name(&d), "two_boys");
        app.finish().await;
    }

    #[tokio::test]
    async fn the_editor_asks_what_a_new_tag_meant() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        app.upload_png(&t, 1, "red_hat cat").await;
        let r = app
            .call(
                Method::POST,
                "/api/v1/tags/cat/aliases",
                Some(&t),
                Some(json!({"alias": "kitty"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        // In the vocabulary but on no post: fine to suggest for tagging.
        patch(&app, &t, "red_hair", json!({"category": "general"})).await;

        let ask = |q: &'static str| {
            let (app, t) = (&app, &t);
            async move {
                let r = app.get(&format!("/api/v1/tag-corrections?q={q}"), t).await;
                assert_eq!(r.status, StatusCode::OK);
                r.json()
            }
        };
        assert_eq!(
            ask("rde_hair+cta").await,
            json!([
                {"from": "rde_hair", "to": "red_hair"},
                {"from": "cta", "to": "cat"},
            ])
        );
        // Known names, aliases, new tags with a category, far-off and empty names.
        for q in [
            "cat+kitty+red_hat",
            "artist:cta",
            "character:rde_hair",
            "qqqqqqqq",
            "",
        ] {
            assert_eq!(ask(q).await, json!([]), "{q}");
        }
        // Spelled as typed, it still gets normalised.
        assert_eq!(ask("CTA").await, json!([{"from": "cta", "to": "cat"}]));
        let r = app.get("/api/v1/tag-corrections?q=cta", "bad-token").await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        app.finish().await;
    }

    #[tokio::test]
    async fn the_vocabulary_is_seeded_once_around_existing_tags() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        // Someone's own arrangement wins: boots stays where it is.
        patch(&app, &t, "boots", json!({"parent": "my_shoes"})).await;
        // A link that would loop is skipped.
        patch(&app, &t, "clothing", json!({"parent": "footwear"})).await;
        assert!(seed_vocab(app.db()).await.unwrap());
        assert!(!seed_vocab(app.db()).await.unwrap());

        let d = app.get("/api/v1/tags/sneakers", &t).await.json();
        let up: Vec<&str> = d["ancestors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        assert_eq!(up, ["footwear"]);
        let d = app.get("/api/v1/tags/boots", &t).await.json();
        assert_eq!(d["ancestors"][0]["name"], "my_shoes");
        let d = app.get("/api/v1/tags/long_hair", &t).await.json();
        assert_eq!(d["ancestors"][0]["name"], "hair");
        assert_eq!(d["ancestors"][1]["name"], "hair_length");
        // The empty starter tags stay out of the tag list.
        let list = app.get("/api/v1/tags", &t).await.json();
        assert_eq!(list, json!([]));
        app.finish().await;
    }
}
