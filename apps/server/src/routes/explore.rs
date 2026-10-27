//! `GET /explore`: trending posts and tags over a window of days, posts added
//! on this date in earlier years, and random picks. A search query narrows
//! every section; its order and `status:` are ignored.

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use serde::Deserialize;
use sqlx::QueryBuilder;
use starbooru_core::{
    DEFAULT_EXPLORE_DAYS, Explore, MAX_EXPLORE_DAYS, Post, TagTrend, parse_query,
};

use super::posts::{PostRow, push_filters, select_posts, tag_ids, to_post};
use super::tags::{TagRow, to_tag};
use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

const TRENDING_LIMIT: i64 = 24;
const TAGS_LIMIT: i64 = 20;
const ON_THIS_DAY_LIMIT: i64 = 12;
const RANDOM_LIMIT: i64 = 12;

#[derive(Deserialize)]
pub struct ExploreParams {
    days: Option<u32>,
    #[serde(default)]
    q: String,
}

pub async fn explore(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ExploreParams>,
) -> AppResult<Json<Explore>> {
    let days = params
        .days
        .unwrap_or(DEFAULT_EXPLORE_DAYS)
        .clamp(1, MAX_EXPLORE_DAYS);
    let mut query = parse_query(&params.q)?;
    query.deleted = false;
    let now = Utc::now();
    let since = now - Duration::days(i64::from(days));

    let (include, wanted) = tag_ids(&state, &query.include).await?;
    if include.len() < wanted {
        // A required tag does not exist, so nothing matches.
        return Ok(Json(Explore {
            days,
            ..Explore::default()
        }));
    }
    let (exclude, _) = tag_ids(&state, &query.exclude).await?;
    let fetch_posts = |mut qb: QueryBuilder<sqlx::Postgres>| {
        let state = state.clone();
        async move {
            let rows = qb.build_query_as::<PostRow>().fetch_all(&state.db).await?;
            AppResult::Ok(
                rows.into_iter()
                    .map(|r| to_post(&state, r))
                    .collect::<Vec<Post>>(),
            )
        }
    };

    let mut qb = select_posts(user.id);
    qb.push(" JOIN (SELECT post_id, sum(value) AS heat FROM post_votes WHERE voted_at >= ")
        .push_bind(since)
        .push(" GROUP BY post_id HAVING sum(value) > 0) h ON h.post_id = p.id");
    push_filters(&mut qb, &query, include.clone(), exclude.clone());
    qb.push(" ORDER BY h.heat DESC, p.score DESC, p.id DESC LIMIT ")
        .push_bind(TRENDING_LIMIT);
    let trending = fetch_posts(qb).await?;

    let mut qb = QueryBuilder::new(
        "SELECT t.id, t.name, t.category, t.post_count, count(*) AS recent \
         FROM posts p CROSS JOIN LATERAL unnest(p.tag_ids) AS u(tag_id) \
         JOIN tags t ON t.id = u.tag_id",
    );
    push_filters(&mut qb, &query, include.clone(), exclude.clone());
    qb.push(" AND p.created_at >= ").push_bind(since);
    if !include.is_empty() {
        // Every match carries the searched tags; show what goes with them.
        qb.push(" AND NOT (u.tag_id = ANY(")
            .push_bind(include.clone())
            .push("))");
    }
    qb.push(" GROUP BY t.id ORDER BY recent DESC, t.name LIMIT ")
        .push_bind(TAGS_LIMIT);
    let rows: Vec<(i64, String, i16, i32, i64)> = qb.build_query_as().fetch_all(&state.db).await?;
    let tags = rows
        .into_iter()
        .map(|(id, name, category, post_count, recent)| TagTrend {
            tag: to_tag((id, name, category, post_count) as TagRow),
            recent,
        })
        .collect();

    let mut qb = select_posts(user.id);
    push_filters(&mut qb, &query, include.clone(), exclude.clone());
    qb.push(" AND EXTRACT(MONTH FROM p.created_at AT TIME ZONE 'UTC')::int = ")
        .push_bind(i32::try_from(now.month()).unwrap_or(1))
        .push(" AND EXTRACT(DAY FROM p.created_at AT TIME ZONE 'UTC')::int = ")
        .push_bind(i32::try_from(now.day()).unwrap_or(1))
        .push(" AND p.created_at < ")
        .push_bind(start_of_year(now))
        .push(" ORDER BY p.created_at DESC, p.id DESC LIMIT ")
        .push_bind(ON_THIS_DAY_LIMIT);
    let on_this_day = fetch_posts(qb).await?;

    let mut qb = select_posts(user.id);
    push_filters(&mut qb, &query, include, exclude);
    qb.push(" ORDER BY random() LIMIT ").push_bind(RANDOM_LIMIT);
    let random = fetch_posts(qb).await?;

    Ok(Json(Explore {
        days,
        trending,
        tags,
        on_this_day,
        random,
    }))
}

/// Midnight UTC on 1 January of `now`'s year.
fn start_of_year(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0)
        .single()
        .unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    use super::*;
    use crate::testing;

    #[test]
    fn year_starts_at_midnight_utc() {
        let now = Utc.with_ymd_and_hms(2026, 10, 27, 15, 4, 5).unwrap();
        assert_eq!(
            start_of_year(now),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
        );
    }

    fn ids(v: &serde_json::Value) -> Vec<i64> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_i64().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn explores_trending_tags_and_this_day() {
        let Some(app) = testing::app().await else {
            return;
        };
        let alice = app.register("alice").await;
        let bob = app.register("bob").await;
        let a = app.upload_png(&alice, 1, "cat sky").await;
        let b = app.upload_png(&alice, 2, "cat night").await;
        let c = app.upload_png(&alice, 3, "dog").await;
        let old = app.upload_png(&alice, 4, "cat").await;
        let gone = app.upload_png(&alice, 5, "cat").await;
        let vote = |id: i64, token: &str, value: i64| {
            let token = token.to_owned();
            let app = &app;
            async move {
                let r = app
                    .call(
                        Method::PUT,
                        &format!("/api/v1/posts/{id}/vote"),
                        Some(&token),
                        Some(json!({ "value": value })),
                    )
                    .await;
                assert_eq!(r.status, StatusCode::OK);
            }
        };
        vote(b, &alice, 1).await;
        vote(b, &bob, 1).await;
        vote(a, &bob, 1).await;
        vote(c, &bob, -1).await;
        vote(gone, &bob, 1).await;
        vote(old, &bob, 1).await;
        app.call(
            Method::DELETE,
            &format!("/api/v1/posts/{gone}"),
            Some(&alice),
            None,
        )
        .await;
        // `old` was uploaded on today's date years ago, and voted on last month.
        sqlx::query("UPDATE posts SET created_at = now() - interval '4 years' WHERE id = $1")
            .bind(old)
            .execute(app.db())
            .await
            .unwrap();
        sqlx::query(
            "UPDATE post_votes SET voted_at = now() - interval '30 days' WHERE post_id = $1",
        )
        .bind(old)
        .execute(app.db())
        .await
        .unwrap();

        let e = app.get("/api/v1/explore", &bob).await;
        assert_eq!(e.status, StatusCode::OK);
        let e = e.json();
        assert_eq!(e["days"], 7);
        // b has two votes, a one; c is negative, `gone` is deleted, `old` is stale.
        assert_eq!(ids(&e["trending"]), [b, a]);
        assert_eq!(e["trending"][0]["my_vote"], 1);
        let tags: Vec<(String, i64)> = e["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t["tag"]["name"].as_str().unwrap().to_owned(),
                    t["recent"].as_i64().unwrap(),
                )
            })
            .collect();
        let pairs = |v: &[(&str, i64)]| -> Vec<(String, i64)> {
            v.iter().map(|(n, c)| ((*n).to_owned(), *c)).collect()
        };
        assert_eq!(
            tags,
            pairs(&[("cat", 2), ("dog", 1), ("night", 1), ("sky", 1)])
        );
        assert_eq!(ids(&e["on_this_day"]), [old]);
        let mut random = ids(&e["random"]);
        random.sort_unstable();
        assert_eq!(random, [a, b, c, old]);

        // A query narrows every section, and its own tags drop out of the list.
        let e = app.get("/api/v1/explore?q=cat&days=1", &bob).await.json();
        assert_eq!(e["days"], 1);
        assert_eq!(ids(&e["trending"]), [b, a]);
        assert_eq!(e["tags"][0]["tag"]["name"], "night");
        assert_eq!(e["tags"].as_array().unwrap().len(), 2);
        let e = app.get("/api/v1/explore?q=-cat", &bob).await.json();
        assert_eq!(ids(&e["trending"]), Vec::<i64>::new());
        assert_eq!(ids(&e["random"]), [c]);
        // An unknown tag matches nothing; a bad query is a 400; days are clamped.
        let e = app.get("/api/v1/explore?q=nope", &bob).await.json();
        assert_eq!(e["random"], json!([]));
        let r = app.get("/api/v1/explore?q=rating:z", &bob).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let e = app.get("/api/v1/explore?days=100000", &bob).await.json();
        assert_eq!(e["days"], MAX_EXPLORE_DAYS);
        assert_eq!(ids(&e["trending"]), [b, old, a]);

        // Changing a vote counts as new; repeating it doesn't.
        vote(old, &bob, 1).await;
        let at: DateTime<Utc> =
            sqlx::query_scalar("SELECT voted_at FROM post_votes WHERE post_id = $1")
                .bind(old)
                .fetch_one(app.db())
                .await
                .unwrap();
        assert!(at < Utc::now() - Duration::days(20));
        vote(old, &bob, -1).await;
        vote(old, &bob, 1).await;
        let e = app.get("/api/v1/explore", &bob).await.json();
        assert_eq!(ids(&e["trending"]), [b, old, a]);
        app.finish().await;
    }
}
