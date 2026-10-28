//! Automatic metadata tags (`highres`, `video`, `sound`, ...; the rules are
//! in `starbooru_core::autotag`). Each post is tagged once, at upload or by
//! the startup backfill, and `posts.meta_tagged_at` remembers it, so a tag
//! someone removes doesn't come back.

use sqlx::{Postgres, Transaction};
use starbooru_core::{MediaFacts, MediaKind, TagCategory, meta_tags};
use tracing::warn;

use crate::error::AppResult;
use crate::settings;
use crate::state::AppState;

/// The facts the rules need, from what the server stored or probed.
pub(super) fn facts(
    kind: MediaKind,
    width: Option<i32>,
    height: Option<i32>,
    duration_ms: Option<i64>,
    has_audio: Option<bool>,
) -> MediaFacts {
    let side = |v: Option<i32>| v.and_then(|v| u32::try_from(v).ok());
    MediaFacts {
        kind,
        width: side(width),
        height: side(height),
        duration_ms,
        has_audio,
    }
}

/// Adds the metadata tags for `facts` to a live post that hasn't had them
/// yet, and marks it tagged. New tags are made in the `meta` category; a
/// tag that exists keeps its category, aliases and implications apply.
pub(super) async fn tag_post(
    tx: &mut Transaction<'_, Postgres>,
    post_id: i64,
    facts: &MediaFacts,
) -> AppResult<()> {
    let row: Option<(Vec<i64>, bool)> = sqlx::query_as(
        "SELECT tag_ids, meta_tagged_at IS NOT NULL FROM posts \
         WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(post_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((old_ids, false)) = row else {
        return Ok(());
    };
    let names: Vec<String> = meta_tags(facts).into_iter().map(String::from).collect();
    let mut added = Vec::new();
    if !names.is_empty() {
        sqlx::query(
            "INSERT INTO tags (name, category) SELECT n, $2 FROM unnest($1::text[]) AS n \
             WHERE NOT EXISTS (SELECT 1 FROM tag_aliases a WHERE a.alias = n) \
             ON CONFLICT (name) DO NOTHING",
        )
        .bind(&names)
        .bind(TagCategory::Meta.to_db())
        .execute(&mut **tx)
        .await?;
        let ids = super::tags::resolve_for_post(tx, &names).await?;
        added = ids.into_iter().filter(|id| !old_ids.contains(id)).collect();
    }
    let mut new_ids = old_ids;
    new_ids.extend_from_slice(&added);
    sqlx::query(
        "UPDATE posts SET tag_ids = $2, meta_tagged_at = now(), \
         updated_at = CASE WHEN $3 THEN now() ELSE updated_at END WHERE id = $1",
    )
    .bind(post_id)
    .bind(&new_ids)
    .bind(!added.is_empty())
    .execute(&mut **tx)
    .await?;
    super::posts::adjust_counts(tx, &added, 1).await
}

/// id, sha256, ext, kind, width, height, duration_ms.
type UntaggedRow = (
    i64,
    Vec<u8>,
    String,
    i16,
    Option<i32>,
    Option<i32>,
    Option<i64>,
);

/// At startup: metadata tags for live posts never tagged (uploaded before
/// this existed, or while it was off). Does nothing while it is off.
pub async fn backfill_meta_tags(state: AppState) {
    match settings::current(&state).await {
        Ok(s) if s.auto_meta_tags => {}
        Ok(_) => return,
        Err(e) => {
            warn!("meta tag backfill skipped: {e}");
            return;
        }
    }
    let mut after = 0i64;
    let mut done = 0u64;
    loop {
        let batch: Result<Vec<UntaggedRow>, _> = sqlx::query_as(
            "SELECT id, sha256, ext, kind, width, height, duration_ms FROM posts \
             WHERE meta_tagged_at IS NULL AND deleted_at IS NULL AND id > $1 \
             ORDER BY id LIMIT 100",
        )
        .bind(after)
        .fetch_all(&state.db)
        .await;
        let batch = match batch {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => break,
            Err(e) => {
                warn!("meta tag backfill stopped: {e}");
                return;
            }
        };
        for (id, sha, ext, kind, width, height, duration_ms) in batch {
            after = id;
            let kind = MediaKind::from_db(kind).unwrap_or(MediaKind::Image);
            let has_audio = if kind == MediaKind::Video {
                let st = state.clone();
                let sha_hex = hex::encode(sha);
                tokio::task::spawn_blocking(move || st.storage.has_audio(&sha_hex, &ext))
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };
            let facts = facts(kind, width, height, duration_ms, has_audio);
            match tag_one(&state, id, &facts).await {
                Ok(()) => done += 1,
                Err(e) => warn!("meta tag backfill: post {id}: {e}"),
            }
        }
    }
    if done > 0 {
        tracing::info!("meta tag backfill: tagged {done} posts");
    }
}

async fn tag_one(state: &AppState, id: i64, facts: &MediaFacts) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    tag_post(&mut tx, id, facts).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::http::Method;
    use serde_json::json;

    use super::*;
    use crate::testing;

    async fn tags_of(app: &testing::TestApp, token: &str, id: i64) -> Vec<String> {
        let r = app.get(&format!("/api/v1/posts/{id}"), token).await;
        serde_json::from_value(r.json()["tags"].clone()).unwrap()
    }

    async fn tag_row(app: &testing::TestApp, name: &str) -> Option<(i16, i32)> {
        sqlx::query_as("SELECT category, post_count FROM tags WHERE name = $1")
            .bind(name)
            .fetch_optional(app.db())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn uploads_get_meta_tags_once() {
        let Some(app) = testing::app_with(|c| c.auto_meta_tags = true).await else {
            return;
        };
        let t = app.register("admin").await;
        // An existing tag keeps its category.
        let r = app
            .upload(&t, &testing::png(1600, 40, 1), &[("tags", "cat highres")])
            .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        let big = r.json()["id"].as_i64().unwrap();
        let mut tags = tags_of(&app, &t, big).await;
        tags.sort();
        assert_eq!(tags, ["cat", "highres", "wide_image"]);
        assert_eq!(tag_row(&app, "highres").await, Some((0, 1)));
        assert_eq!(
            tag_row(&app, "wide_image").await,
            Some((TagCategory::Meta.to_db(), 1))
        );

        let small = app.upload_png(&t, 2, "dog").await;
        let mut tags = tags_of(&app, &t, small).await;
        tags.sort();
        assert_eq!(tags, ["dog", "lowres"]);
        assert_eq!(
            tag_row(&app, "lowres").await,
            Some((TagCategory::Meta.to_db(), 1))
        );

        // Someone removes `lowres`; the startup backfill leaves it off.
        let r = app
            .call(
                Method::PATCH,
                &format!("/api/v1/posts/{small}"),
                Some(&t),
                Some(json!({ "tags": ["dog"] })),
            )
            .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        backfill_meta_tags(app.state.clone()).await;
        assert_eq!(tags_of(&app, &t, small).await, ["dog"]);
        assert_eq!(tag_row(&app, "lowres").await.map(|r| r.1), Some(0));

        // A post from before this existed is tagged by the backfill, once.
        sqlx::query("UPDATE posts SET meta_tagged_at = NULL WHERE id = $1")
            .bind(small)
            .execute(app.db())
            .await
            .unwrap();
        backfill_meta_tags(app.state.clone()).await;
        backfill_meta_tags(app.state.clone()).await;
        let mut tags = tags_of(&app, &t, small).await;
        tags.sort();
        assert_eq!(tags, ["dog", "lowres"]);
        assert_eq!(tag_row(&app, "lowres").await.map(|r| r.1), Some(1));
        app.finish().await;
    }

    #[tokio::test]
    async fn the_setting_turns_it_off() {
        let Some(app) = testing::app_with(|c| c.auto_meta_tags = true).await else {
            return;
        };
        let t = app.register("admin").await;
        let r = app.get("/api/v1/admin/settings", &t).await;
        assert_eq!(r.json()["auto_meta_tags"], true);
        let mut settings = r.json();
        settings["auto_meta_tags"] = json!(false);
        let r = app
            .call(
                Method::PUT,
                "/api/v1/admin/settings",
                Some(&t),
                Some(settings),
            )
            .await;
        assert_eq!(r.status, 200, "{:?}", r.json());
        assert_eq!(r.json()["auto_meta_tags"], false);

        let id = app.upload_png(&t, 3, "dog").await;
        assert_eq!(tags_of(&app, &t, id).await, ["dog"]);
        backfill_meta_tags(app.state.clone()).await;
        assert_eq!(tags_of(&app, &t, id).await, ["dog"]);
        assert_eq!(tag_row(&app, "lowres").await, None);
        let tagged: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT meta_tagged_at FROM posts WHERE id = $1")
                .bind(id)
                .fetch_one(app.db())
                .await
                .unwrap();
        assert!(tagged.is_none());
        app.finish().await;
    }

    #[tokio::test]
    async fn aliases_and_implications_apply() {
        let Some(app) = testing::app_with(|c| c.auto_meta_tags = true).await else {
            return;
        };
        let t = app.register("admin").await;
        // `lowres` is an alias of `small_image`, which implies `tiny`.
        sqlx::query("INSERT INTO tags (name) VALUES ('small_image'), ('tiny')")
            .execute(app.db())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tag_aliases (alias, tag_id) \
             SELECT 'lowres', id FROM tags WHERE name = 'small_image'",
        )
        .execute(app.db())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tag_implications (tag_id, implied_id) SELECT a.id, b.id \
             FROM tags a, tags b WHERE a.name = 'small_image' AND b.name = 'tiny'",
        )
        .execute(app.db())
        .await
        .unwrap();
        let id = app.upload_png(&t, 4, "").await;
        let mut tags = tags_of(&app, &t, id).await;
        tags.sort();
        assert_eq!(tags, ["small_image", "tiny"]);
        assert_eq!(tag_row(&app, "lowres").await, None);
        assert_eq!(tag_row(&app, "tiny").await.map(|r| r.1), Some(1));
        app.finish().await;
    }

    #[test]
    fn facts_ignore_negative_sizes() {
        let f = facts(MediaKind::Image, Some(-1), Some(10), None, None);
        assert_eq!((f.width, f.height), (None, Some(10)));
    }
}
