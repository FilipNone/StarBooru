//! The people in a post ("figures", `starbooru_core::figure`): saving them,
//! loading them for the viewer, and keeping them in step with the post's
//! tags. A figure's tags are on the post too; taking a tag off the post
//! takes it off the figures, and while a post has figures its count tags
//! (`1girl`, `solo`...) are worked out from them.

use axum::Json;
use axum::extract::{Path, State};
use sqlx::{PgExecutor, Postgres, Transaction};
use starbooru_core::tag::{MAX_TAGS_PER_POST, TagError};
use starbooru_core::{
    COSPLAY_TAG, Figure, FigureKind, FiguresUpdate, Post, Slot, TagCategory, clean_figures,
    cosplay_name, cosplay_tag, required_tags,
};

use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

/// `figure_tags.slot` of the character tag; the slots proper start at 2.
const CHARACTER_SLOT: i16 = 0;
/// `figure_tags.slot` of the `name_(cosplay)` tag.
const COSPLAY_SLOT: i16 = 1;

/// A post's figures, in order.
pub(super) async fn load(db: impl PgExecutor<'_>, post_id: i64) -> AppResult<Vec<Figure>> {
    let rows: Vec<(i16, i16, Option<i16>, Option<String>)> = sqlx::query_as(
        "SELECT f.position, f.kind, ft.slot, t.name FROM figures f \
         LEFT JOIN figure_tags ft ON ft.figure_id = f.id \
         LEFT JOIN tags t ON t.id = ft.tag_id \
         WHERE f.post_id = $1 ORDER BY f.position, ft.slot, ft.position",
    )
    .bind(post_id)
    .fetch_all(db)
    .await?;
    let mut out: Vec<(i16, Figure)> = Vec::new();
    for (position, kind, slot, name) in rows {
        if out.last().is_none_or(|(p, _)| *p != position) {
            out.push((
                position,
                Figure {
                    kind: FigureKind::from_db(kind).unwrap_or_default(),
                    ..Figure::default()
                },
            ));
        }
        let (Some((_, figure)), Some(slot), Some(name)) = (out.last_mut(), slot, name) else {
            continue;
        };
        match slot {
            CHARACTER_SLOT => figure.character = Some(name),
            COSPLAY_SLOT => figure.cosplay = Some(cosplay_name(&name).to_owned()),
            slot => {
                let slot = Slot::from_db(slot).unwrap_or(Slot::Other);
                figure.tags.entry(slot).or_default().push(name);
            }
        }
    }
    Ok(out.into_iter().map(|(_, f)| f).collect())
}

/// Takes tags off a post's figures that the post no longer has.
pub(super) async fn prune(
    tx: &mut Transaction<'_, Postgres>,
    post_id: i64,
    tag_ids: &[i64],
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM figure_tags ft USING figures f \
         WHERE ft.figure_id = f.id AND f.post_id = $1 AND NOT (ft.tag_id = ANY($2))",
    )
    .bind(post_id)
    .bind(tag_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Every tag a post's figures put on it, the count tags included.
async fn derived(tx: &mut Transaction<'_, Postgres>, post_id: i64) -> AppResult<Vec<String>> {
    let figures = load(&mut **tx, post_id).await?;
    let mut names: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM figures f JOIN figure_tags ft ON ft.figure_id = f.id \
         JOIN tags t ON t.id = ft.tag_id WHERE f.post_id = $1",
    )
    .bind(post_id)
    .fetch_all(&mut **tx)
    .await?;
    names.extend(required_tags(&figures));
    Ok(names)
}

/// Replaces a live, locked post's figures and updates its tags to match:
/// tags only the old figures gave are removed, the new figures' tags are
/// added. Names are resolved through aliases and missing tags created;
/// character tags become characters, and new `name_(cosplay)` tags go
/// under `cosplay`.
pub(crate) async fn replace(
    tx: &mut Transaction<'_, Postgres>,
    post_id: i64,
    figures: Vec<Figure>,
) -> AppResult<()> {
    let figures = clean_figures(figures)?;
    let old = derived(tx, post_id).await?;
    let names: Vec<String> = figures.iter().flat_map(Figure::tag_names).collect();
    let tags = super::tags::by_name(tx, &names).await?;
    let id_of = |name: &str| tags.get(name).map(|(id, _)| *id);

    sqlx::query("DELETE FROM figures WHERE post_id = $1")
        .bind(post_id)
        .execute(&mut **tx)
        .await?;
    let mut characters = Vec::new();
    let mut cosplays = Vec::new();
    for (position, figure) in (0i16..).zip(&figures) {
        let figure_id: i64 = sqlx::query_scalar(
            "INSERT INTO figures (post_id, position, kind) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(post_id)
        .bind(position)
        .bind(figure.kind.to_db())
        .fetch_one(&mut **tx)
        .await?;
        let mut rows: Vec<(i64, i16)> = Vec::new();
        let character = figure.character.as_deref().and_then(id_of);
        let cosplay = figure.cosplay.as_deref().map(cosplay_tag);
        let cosplay = cosplay.as_deref().and_then(id_of);
        rows.extend(character.map(|id| (id, CHARACTER_SLOT)));
        rows.extend(cosplay.map(|id| (id, COSPLAY_SLOT)));
        characters.extend(character);
        cosplays.extend(cosplay);
        for (slot, names) in &figure.tags {
            rows.extend(
                names
                    .iter()
                    .filter_map(|n| id_of(n))
                    .map(|id| (id, slot.to_db())),
            );
        }
        // Two names can be aliases of one tag: the first place wins.
        let mut seen = Vec::with_capacity(rows.len());
        rows.retain(|(id, _)| {
            let new = !seen.contains(id);
            seen.push(*id);
            new
        });
        let (ids, slots): (Vec<i64>, Vec<i16>) = rows.into_iter().unzip();
        sqlx::query(
            "INSERT INTO figure_tags (figure_id, tag_id, slot, position) \
             SELECT $1, t.id, t.slot, (t.ord - 1)::smallint \
             FROM unnest($2::bigint[], $3::smallint[]) WITH ORDINALITY AS t(id, slot, ord)",
        )
        .bind(figure_id)
        .bind(&ids)
        .bind(&slots)
        .execute(&mut **tx)
        .await?;
    }
    if !characters.is_empty() {
        sqlx::query("UPDATE tags SET category = $2 WHERE id = ANY($1) AND category = $3")
            .bind(&characters)
            .bind(TagCategory::Character.to_db())
            .bind(TagCategory::General.to_db())
            .execute(&mut **tx)
            .await?;
    }
    let parent = if cosplays.is_empty() {
        None
    } else {
        let found = super::tags::by_name(tx, &[COSPLAY_TAG.to_owned()]).await?;
        found.get(COSPLAY_TAG).map(|(id, _)| *id)
    };
    if let Some(parent) = parent {
        // Under `cosplay` unless placed already, or `cosplay` sits below it.
        sqlx::query(
            "WITH RECURSIVE c AS (SELECT id, parent_id, 0 AS depth FROM tags WHERE id = $2 \
                 UNION ALL SELECT t.id, t.parent_id, c.depth + 1 FROM tags t \
                 JOIN c ON t.id = c.parent_id WHERE c.depth < 32) \
             UPDATE tags SET parent_id = $2 \
             WHERE id = ANY($1) AND parent_id IS NULL AND id NOT IN (SELECT id FROM c)",
        )
        .bind(&cosplays)
        .bind(parent)
        .execute(&mut **tx)
        .await?;
    }

    // The post's tags: what only the old figures gave goes, the new figures' come.
    let new = derived(tx, post_id).await?;
    let gone: Vec<String> = old.into_iter().filter(|n| !new.contains(n)).collect();
    let current: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM posts p JOIN tags t ON t.id = ANY(p.tag_ids) WHERE p.id = $1",
    )
    .bind(post_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut names: Vec<String> = current.into_iter().filter(|n| !gone.contains(n)).collect();
    for name in new {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.len() > MAX_TAGS_PER_POST {
        return Err(TagError::TooMany.into());
    }
    super::posts::set_tags(tx, post_id, &names).await
}

/// `PUT /posts/{id}/figures`: replaces the post's figures and returns it.
pub async fn put(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(update): Json<FiguresUpdate>,
) -> AppResult<Json<Post>> {
    let figures = clean_figures(update.figures)?;
    let mut tx = state.db.begin().await?;
    super::posts::lock_live(&mut tx, id).await?;
    replace(&mut tx, id, figures).await?;
    tx.commit().await?;
    Ok(Json(super::posts::load_full(&state, user.id, id).await?))
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use serde_json::{Value, json};

    use crate::testing::{self, TestApp};

    async fn put_figures(app: &TestApp, t: &str, id: i64, figures: Value) -> testing::Resp {
        app.call(
            Method::PUT,
            &format!("/api/v1/posts/{id}/figures"),
            Some(t),
            Some(json!({ "figures": figures })),
        )
        .await
    }

    fn sorted_tags(post: &Value) -> Vec<String> {
        let mut tags: Vec<String> = post["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect();
        tags.sort();
        tags
    }

    async fn found(app: &TestApp, t: &str, q: &str) -> Vec<i64> {
        let r = app.get(&format!("/api/v1/posts?q={q}"), t).await;
        assert_eq!(r.status, StatusCode::OK, "{q}: {:?}", r.json());
        let mut ids: Vec<i64> = r.json()["posts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_i64().unwrap())
            .collect();
        ids.sort_unstable();
        ids
    }

    #[tokio::test]
    async fn figures_keep_the_post_tags_in_step() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let id = app.upload_png(&t, 1, "sky 2girls solo").await;
        let figures = json!([
            {"kind": "girl", "character": "char:Saber",
             "tags": {"hair": ["blonde_hair"], "shoes": ["boots"]}},
            {"kind": "boy", "cosplay": "link_(cosplay)", "tags": {"head": ["hat"]}},
        ]);
        let r = put_figures(&app, &t, id, figures).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let post = r.json();
        assert_eq!(
            sorted_tags(&post),
            [
                "1boy",
                "1girl",
                "blonde_hair",
                "boots",
                "cosplay",
                "hat",
                "link_(cosplay)",
                "saber",
                "sky"
            ]
        );
        assert_eq!(
            post["figures"],
            json!([
                {"kind": "girl", "character": "saber",
                 "tags": {"hair": ["blonde_hair"], "shoes": ["boots"]}},
                {"kind": "boy", "cosplay": "link", "tags": {"head": ["hat"]}},
            ])
        );
        let saber = app.get("/api/v1/tags/saber", &t).await.json();
        assert_eq!(saber["tag"]["category"], "character");
        let link = app.get("/api/v1/tags/link_(cosplay)", &t).await.json();
        assert_eq!(link["ancestors"][0]["name"], "cosplay");

        // Taking a tag off the post takes it off its figure; the counts come back.
        let r = app
            .call(
                Method::PATCH,
                &format!("/api/v1/posts/{id}"),
                Some(&t),
                Some(json!({"tags": ["sky", "saber", "blonde_hair", "hat", "link_(cosplay)"]})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let post = r.json();
        assert!(!sorted_tags(&post).contains(&"boots".to_owned()));
        assert!(sorted_tags(&post).contains(&"1girl".to_owned()));
        assert!(sorted_tags(&post).contains(&"cosplay".to_owned()));
        assert_eq!(post["figures"][0]["tags"], json!({"hair": ["blonde_hair"]}));

        // One figure left: solo, and what only the boy gave goes.
        let r = put_figures(&app, &t, id, json!([post["figures"][0].clone()])).await;
        assert_eq!(
            sorted_tags(&r.json()),
            ["1girl", "blonde_hair", "saber", "sky", "solo"]
        );
        // None left: back to the post's own tags.
        let r = put_figures(&app, &t, id, json!([])).await;
        assert_eq!(sorted_tags(&r.json()), ["sky"]);
        assert!(r.json().get("figures").is_none());
        // Without figures, count tags are ordinary tags.
        let r = app
            .call(
                Method::PATCH,
                &format!("/api/v1/posts/{id}"),
                Some(&t),
                Some(json!({"tags": ["sky", "3girls"]})),
            )
            .await;
        assert_eq!(sorted_tags(&r.json()), ["3girls", "sky"]);

        // Limits, and posts in the bin.
        let many = json!(vec![
            json!({"kind": "other"});
            starbooru_core::MAX_FIGURES + 1
        ]);
        let r = put_figures(&app, &t, id, many).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = put_figures(
            &app,
            &t,
            id,
            json!([{"kind": "girl", "tags": {"hair": ["-x"]}}]),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = put_figures(&app, &t, 999_999, json!([])).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/posts/{id}"),
                Some(&t),
                None,
            )
            .await;
        assert!(r.status.is_success());
        let r = put_figures(&app, &t, id, json!([{"kind": "girl"}])).await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        app.finish().await;
    }

    #[tokio::test]
    async fn searches_find_one_figure_with_every_tag() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let together = app.upload_png(&t, 1, "").await;
        let apart = app.upload_png(&t, 2, "").await;
        let plain = app.upload_png(&t, 3, "blonde_hair boots").await;
        let r = put_figures(
            &app,
            &t,
            together,
            json!([{"kind": "girl", "tags": {"hair": ["blonde_hair"], "shoes": ["boots"]}}]),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK);
        let r = put_figures(
            &app,
            &t,
            apart,
            json!([
                {"kind": "girl", "tags": {"hair": ["blonde_hair"]}},
                {"kind": "boy", "tags": {"shoes": ["boots"]}},
            ]),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK);
        let r = app
            .call(
                Method::PATCH,
                "/api/v1/tags/boots",
                Some(&t),
                Some(json!({"parent": "footwear"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);

        // Plain tags still find every post that has them.
        assert_eq!(
            found(&app, &t, "blonde_hair+boots").await,
            [together, apart, plain]
        );
        assert_eq!(found(&app, &t, "%7Bblonde_hair+boots%7D").await, [together]);
        // A parent stands for its tree; a kind narrows it down.
        assert_eq!(
            found(&app, &t, "%7Bgirl+footwear+blonde_hair%7D").await,
            [together]
        );
        assert_eq!(found(&app, &t, "%7Bboy+footwear%7D").await, [apart]);
        assert_eq!(found(&app, &t, "%7Bboy%7D").await, [apart]);
        assert_eq!(found(&app, &t, "%7Bother%7D").await, Vec::<i64>::new());
        // Exclusion: no figure matches.
        assert_eq!(
            found(&app, &t, "-%7Bblonde_hair+boots%7D").await,
            [apart, plain]
        );
        // Unknown tags: nothing matches, and nothing is excluded.
        assert_eq!(
            found(&app, &t, "%7Bblonde_hair+nowhere%7D").await,
            Vec::<i64>::new()
        );
        assert_eq!(
            found(&app, &t, "-%7Bnowhere%7D").await,
            [together, apart, plain]
        );
        let r = app.get("/api/v1/posts?q=%7B%7D", &t).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        app.finish().await;
    }

    #[tokio::test]
    async fn an_edit_keeps_the_people() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let id = app.upload_png(&t, 1, "sky").await;
        let figures = json!([{"kind": "boy", "tags": {"hair": ["blonde_hair"]}}]);
        assert_eq!(put_figures(&app, &t, id, figures.clone()).await.status, 200);
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/posts/{id}/edit"),
                Some(&t),
                Some(json!({"rotate": 90})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let copy = r.json();
        assert_ne!(copy["id"].as_i64(), Some(id));
        assert_eq!(copy["figures"], figures);
        assert_eq!(sorted_tags(&copy), ["1boy", "blonde_hair", "sky", "solo"]);
        app.finish().await;
    }

    #[tokio::test]
    async fn a_post_stays_under_the_tag_limit() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let id = app.upload_png(&t, 1, "sky").await;
        let figures: Vec<Value> = (0..20)
            .map(|f| {
                let tags: Vec<String> = (0..30).map(|i| format!("t{f}_{i}")).collect();
                json!({"kind": "other", "tags": {"other": tags}})
            })
            .collect();
        let r = put_figures(&app, &t, id, json!(figures)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json());
        // Nothing was saved.
        let post = app.get(&format!("/api/v1/posts/{id}"), &t).await.json();
        assert_eq!(sorted_tags(&post), ["sky"]);
        assert!(post.get("figures").is_none());
        app.finish().await;
    }

    #[tokio::test]
    async fn merging_an_alias_moves_figure_tags() {
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let id = app.upload_png(&t, 1, "").await;
        let r = put_figures(
            &app,
            &t,
            id,
            json!([{"kind": "girl", "tags": {"hair": ["blond_hair", "long_hair"]}}]),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK);
        let r = app
            .call(
                Method::POST,
                "/api/v1/tags/blonde_hair/aliases",
                Some(&t),
                Some(json!({"alias": "blond_hair"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let post = app.get(&format!("/api/v1/posts/{id}"), &t).await.json();
        assert_eq!(
            post["figures"][0]["tags"]["hair"],
            json!(["blonde_hair", "long_hair"])
        );
        // An alias given when saving is followed too.
        let r = put_figures(
            &app,
            &t,
            id,
            json!([{"kind": "girl", "tags": {"hair": ["blond_hair", "blonde_hair"]}}]),
        )
        .await;
        assert_eq!(
            r.json()["figures"][0]["tags"]["hair"],
            json!(["blonde_hair"])
        );
        assert_eq!(sorted_tags(&r.json()), ["1girl", "blonde_hair", "solo"]);
        app.finish().await;
    }
}
