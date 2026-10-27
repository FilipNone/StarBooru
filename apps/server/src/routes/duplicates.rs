//! The near-duplicate finder: `GET /duplicates` groups live posts whose
//! thumbnails look alike (see `similar`), `POST /duplicates/resolve` keeps one
//! post of a group and moves the rest to the recycle bin, and
//! `POST /duplicates/dismiss` marks a group as not duplicates.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use starbooru_core::tag::{MAX_TAGS_PER_POST, TagError};
use starbooru_core::{
    DEFAULT_DUPLICATE_DISTANCE, DismissDuplicates, DuplicateGroup, Duplicates,
    MAX_DUPLICATE_DISTANCE, MAX_DUPLICATE_GROUP, Post, ResolveDuplicates,
};

use super::posts::{PostRow, adjust_counts, load, select_posts, set_tags, to_post};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::similar::{self, Fingerprint};
use crate::state::AppState;

const DEFAULT_GROUPS: u32 = 30;
const MAX_GROUPS: u32 = 100;

#[derive(Deserialize)]
pub struct DuplicateParams {
    distance: Option<u32>,
    limit: Option<u32>,
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<DuplicateParams>,
) -> AppResult<Json<Duplicates>> {
    let distance = params
        .distance
        .unwrap_or(DEFAULT_DUPLICATE_DISTANCE)
        .min(MAX_DUPLICATE_DISTANCE);
    let limit = params.limit.unwrap_or(DEFAULT_GROUPS).clamp(1, MAX_GROUPS) as usize;

    let prints: Vec<(i64, i64, i32)> = sqlx::query_as(
        "SELECT id, phash, pcolor FROM posts \
         WHERE deleted_at IS NULL AND phash IS NOT NULL AND pcolor IS NOT NULL ORDER BY id",
    )
    .fetch_all(&state.db)
    .await?;
    let unhashed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM posts \
             WHERE deleted_at IS NULL AND (phash IS NULL OR pcolor IS NULL)",
    )
    .fetch_one(&state.db)
    .await?;
    let dismissed: Vec<(i64, i64)> =
        sqlx::query_as("SELECT post_a, post_b FROM duplicate_dismissals")
            .fetch_all(&state.db)
            .await?;

    let groups = tokio::task::spawn_blocking(move || {
        let items: Vec<(i64, Fingerprint)> = prints
            .into_iter()
            .map(|(id, hash, color)| (id, Fingerprint::from_db(hash, color)))
            .collect();
        let dismissed: HashSet<(i64, i64)> = dismissed.into_iter().collect();
        similar::group(&items, distance, &dismissed)
    })
    .await?;
    let total = groups.len() as u64;
    let groups: Vec<similar::Group> = groups.into_iter().take(limit).collect();

    let wanted: Vec<i64> = groups
        .iter()
        .flat_map(|g| g.ids.iter().take(MAX_DUPLICATE_GROUP).copied())
        .collect();
    let mut qb = select_posts(user.id);
    qb.push(" WHERE p.id = ANY(").push_bind(wanted).push(")");
    let rows = qb.build_query_as::<PostRow>().fetch_all(&state.db).await?;
    let mut posts: HashMap<i64, Post> = rows
        .into_iter()
        .map(|r| {
            let post = to_post(&state, r);
            (post.id, post)
        })
        .collect();

    let groups = groups
        .into_iter()
        .map(|g| {
            let mut members: Vec<Post> = g
                .ids
                .iter()
                .take(MAX_DUPLICATE_GROUP)
                .filter_map(|id| posts.remove(id))
                .collect();
            members.sort_by_key(|p| {
                let pixels = i64::from(p.width.unwrap_or(0)) * i64::from(p.height.unwrap_or(0));
                (
                    std::cmp::Reverse(pixels),
                    std::cmp::Reverse(p.file_size),
                    p.id,
                )
            });
            DuplicateGroup {
                distance: g.distance,
                size: u32::try_from(g.ids.len()).unwrap_or(u32::MAX),
                posts: members,
            }
        })
        .collect();
    Ok(Json(Duplicates {
        distance,
        total,
        unhashed,
        groups,
    }))
}

/// Keeps `keep`, optionally merging the others' tags (and a missing source)
/// into it, and moves the others to the recycle bin, all in one transaction.
pub async fn resolve(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ResolveDuplicates>,
) -> AppResult<Json<Post>> {
    let mut remove = req.remove.clone();
    remove.sort_unstable();
    remove.dedup();
    if remove.is_empty() || remove.len() >= MAX_DUPLICATE_GROUP {
        return Err(AppError::BadRequest(format!(
            "remove between 1 and {} posts",
            MAX_DUPLICATE_GROUP - 1
        )));
    }
    if remove.contains(&req.keep) {
        return Err(AppError::BadRequest(
            "the post to keep can't also be removed".into(),
        ));
    }
    let mut all = remove.clone();
    all.push(req.keep);
    all.sort_unstable();

    let mut tx = state.db.begin().await?;
    // Locked in id order, so concurrent edits can't deadlock.
    // (id, uploader_id, tag_ids, source)
    type Row = (i64, Option<i64>, Vec<i64>, Option<String>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, uploader_id, tag_ids, source FROM posts \
         WHERE id = ANY($1) AND deleted_at IS NULL ORDER BY id FOR UPDATE",
    )
    .bind(&all)
    .fetch_all(&mut *tx)
    .await?;
    if rows.len() != all.len() {
        return Err(AppError::NotFound);
    }
    let removed: Vec<&Row> = rows.iter().filter(|r| r.0 != req.keep).collect();
    if removed.iter().any(|r| !user.can_moderate(r.1)) {
        return Err(AppError::Forbidden);
    }

    if req.merge_tags {
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT t.name FROM posts p CROSS JOIN LATERAL unnest(p.tag_ids) AS u(id) \
             JOIN tags t ON t.id = u.id WHERE p.id = ANY($1) ORDER BY t.name",
        )
        .bind(&all)
        .fetch_all(&mut *tx)
        .await?;
        if names.len() > MAX_TAGS_PER_POST {
            return Err(TagError::TooMany.into());
        }
        set_tags(&mut tx, req.keep, &names).await?;
        let kept_source = rows
            .iter()
            .find(|r| r.0 == req.keep)
            .and_then(|r| r.3.as_ref());
        let other_source = removed.iter().find_map(|r| r.3.as_ref());
        if let (None, Some(source)) = (kept_source, other_source) {
            sqlx::query("UPDATE posts SET source = $2 WHERE id = $1")
                .bind(req.keep)
                .bind(source)
                .execute(&mut *tx)
                .await?;
        }
    }

    sqlx::query("UPDATE posts SET deleted_at = now(), updated_at = now() WHERE id = ANY($1)")
        .bind(&remove)
        .execute(&mut *tx)
        .await?;
    for (_, _, tag_ids, _) in &removed {
        adjust_counts(&mut tx, tag_ids, -1).await?;
    }
    tx.commit().await?;
    Ok(Json(load(&state, user.id, req.keep).await?))
}

/// Remembers every pair of `ids` as "not duplicates".
pub async fn dismiss(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(req): Json<DismissDuplicates>,
) -> AppResult<StatusCode> {
    let mut ids = req.ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() < 2 || ids.len() > MAX_DUPLICATE_GROUP {
        return Err(AppError::BadRequest(format!(
            "dismiss between 2 and {MAX_DUPLICATE_GROUP} posts"
        )));
    }
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for (i, first) in ids.iter().enumerate() {
        for second in ids.iter().skip(i + 1) {
            a.push(*first);
            b.push(*second);
        }
    }
    sqlx::query(
        "INSERT INTO duplicate_dismissals (post_a, post_b) \
         SELECT t.a, t.b FROM UNNEST($1::bigint[], $2::bigint[]) AS t(a, b) \
         JOIN posts pa ON pa.id = t.a JOIN posts pb ON pb.id = t.b \
         ON CONFLICT DO NOTHING",
    )
    .bind(&a)
    .bind(&b)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use axum::http::{Method, StatusCode};
    use image::imageops::FilterType;
    use serde_json::{Value, json};

    use crate::similar::tests::pattern;
    use crate::testing::{self, TestApp};

    fn encode(img: &image::DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        img.to_rgb8().write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    async fn upload(app: &TestApp, token: &str, bytes: &[u8], tags: &str, source: &str) -> i64 {
        let r = app
            .upload(token, bytes, &[("tags", tags), ("source", source)])
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        r.json()["id"].as_i64().unwrap()
    }

    fn group_ids(v: &Value) -> Vec<Vec<i64>> {
        v["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| {
                g["posts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["id"].as_i64().unwrap())
                    .collect()
            })
            .collect()
    }

    #[tokio::test]
    async fn finds_resolves_and_dismisses_duplicates() {
        let Some(app) = testing::app().await else {
            return;
        };
        let alice = app.register("alice").await;
        let bob = app.register("bob").await;
        let big = pattern(2, 800, 600);
        let original = upload(
            &app,
            &alice,
            &encode(&big, image::ImageFormat::Png),
            "cat",
            "",
        )
        .await;
        let small = big.resize_exact(400, 300, FilterType::Lanczos3);
        let copy = upload(
            &app,
            &alice,
            &encode(&small, image::ImageFormat::Jpeg),
            "cat sky",
            "https://example.com/a",
        )
        .await;
        let other = upload(
            &app,
            &alice,
            &encode(&pattern(5, 800, 600), image::ImageFormat::Png),
            "dog",
            "",
        )
        .await;
        let other_copy = upload(
            &app,
            &bob,
            &encode(&pattern(5, 640, 480), image::ImageFormat::Png),
            "dog night",
            "",
        )
        .await;

        let d = app.get("/api/v1/duplicates", &alice).await;
        assert_eq!(d.status, StatusCode::OK);
        let d = d.json();
        assert_eq!(d["distance"], 6);
        assert_eq!(d["unhashed"], 0);
        assert_eq!(d["total"], 2);
        let mut groups = group_ids(&d);
        groups.sort();
        // The larger picture comes first in each group.
        assert_eq!(groups, [vec![original, copy], vec![other, other_copy]]);
        // `limit` caps the list, not the total; distance is capped.
        let one = app.get("/api/v1/duplicates?limit=1", &alice).await.json();
        assert_eq!((one["total"].as_u64(), group_ids(&one).len()), (Some(2), 1));
        let huge = app
            .get("/api/v1/duplicates?distance=999", &alice)
            .await
            .json();
        assert_eq!(huge["distance"], 10);

        // Bob isn't an admin, so he can't move Alice's post to the bin.
        let r = app
            .call(
                Method::POST,
                "/api/v1/duplicates/resolve",
                Some(&bob),
                Some(json!({"keep": other_copy, "remove": [other], "merge_tags": true})),
            )
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        // Bad requests.
        for body in [
            json!({"keep": original, "remove": []}),
            json!({"keep": original, "remove": [original]}),
        ] {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/duplicates/resolve",
                    Some(&alice),
                    Some(body),
                )
                .await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST);
        }
        let r = app
            .call(
                Method::POST,
                "/api/v1/duplicates/resolve",
                Some(&alice),
                Some(json!({"keep": original, "remove": [999_999]})),
            )
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);

        // Keep the original, merging tags and the missing source.
        let r = app
            .call(
                Method::POST,
                "/api/v1/duplicates/resolve",
                Some(&alice),
                Some(json!({"keep": original, "remove": [copy], "merge_tags": true})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let kept = r.json();
        assert_eq!(kept["tags"], json!(["cat", "sky"]));
        assert_eq!(kept["source"], "https://example.com/a");
        let gone = app
            .get(&format!("/api/v1/posts/{copy}"), &alice)
            .await
            .json();
        assert_eq!(gone["deleted"], true);
        let tags = app.get("/api/v1/tags?q=sky", &alice).await.json();
        assert_eq!(tags[0]["post_count"], 1);

        // Bob says his pair isn't a duplicate; it stops showing.
        let r = app
            .call(
                Method::POST,
                "/api/v1/duplicates/dismiss",
                Some(&bob),
                Some(json!({"ids": [other_copy, other, 123_456]})),
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        let d = app.get("/api/v1/duplicates", &alice).await.json();
        assert_eq!(d["total"], 0);
        let r = app
            .call(
                Method::POST,
                "/api/v1/duplicates/dismiss",
                Some(&bob),
                Some(json!({"ids": [other]})),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);

        // Posts uploaded before fingerprints existed are hashed at startup.
        sqlx::query("UPDATE posts SET phash = NULL, pcolor = NULL")
            .execute(app.db())
            .await
            .unwrap();
        let d = app.get("/api/v1/duplicates", &alice).await.json();
        assert_eq!(d["unhashed"], 3);
        crate::routes::backfill_hashes(app.state.clone()).await;
        let d = app.get("/api/v1/duplicates", &alice).await.json();
        assert_eq!(d["unhashed"], 0);
        app.finish().await;
    }
}
