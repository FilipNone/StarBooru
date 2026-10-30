//! Search speed at scale: seeds a collection of generated posts straight
//! into the database (one million by default) and times the API calls the
//! clients make most, through the real router. Ignored by default:
//!
//! ```sh
//! STARBOORU_TEST_DATABASE_URL=postgres://… \
//!   cargo test --release -p starbooru-server bench -- --ignored --nocapture
//! ```
//!
//! `STARBOORU_BENCH_POSTS` changes the size. With `STARBOORU_BENCH_KEEP` set,
//! the seeded database is left behind (its name is printed) for `EXPLAIN`.
//! Results are in `docs/development.md`.

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use serde_json::json;

use crate::testing;

/// Distinct tags. Popularity falls off steeply, as in a real collection:
/// tag 1 is on about a third of the posts, the last ones on a handful.
const TAGS: i64 = 5_000;
/// Timed runs per call, after one warm-up.
const RUNS: usize = 5;
/// The slowest median any call may have. Generous, so it catches a bad plan
/// (a full scan or sort of the table) rather than a slow machine.
const LIMIT: Duration = Duration::from_millis(400);

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times.get(times.len() / 2).copied().unwrap_or_default()
}

#[tokio::test]
#[ignore = "seeds a million posts; run by hand"]
async fn search_at_scale() {
    let posts: i64 = std::env::var("STARBOORU_BENCH_POSTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000);
    let Some(app) = testing::app().await else {
        return;
    };
    let token = app.register("admin").await;
    let db = app.db();

    let seeding = Instant::now();
    sqlx::query(
        "INSERT INTO tags (name, category) \
         SELECT 'tag_' || n, (n % 5)::smallint FROM generate_series(1, $1) n",
    )
    .bind(TAGS)
    .execute(db)
    .await
    .unwrap();
    // 5 to 24 tags a post, drawn towards the popular ones. The inner series
    // refers to `g`, so it runs again for every post. Hashes are random,
    // except for one post in 100 that is blank (hash 0, as flat pictures
    // get), which is the worst case for finding look-alikes.
    sqlx::query(
        "INSERT INTO posts (sha256, ext, mime, kind, file_size, width, height, has_thumb, \
                            rating, score, tag_ids, uploader_id, created_at, updated_at, deleted_at, phash) \
         SELECT sha256(int8send(g)), 'png', 'image/png', \
                CASE WHEN g % 25 = 0 THEN 2 WHEN g % 40 = 0 THEN 1 ELSE 0 END, \
                (10000 + random() * 20000000)::bigint, \
                (200 + random() * 4000)::int, (200 + random() * 4000)::int, true, \
                (g % 4)::smallint, (random() * 50 - 5)::int, \
                (SELECT array_agg(DISTINCT t) FROM \
                   (SELECT 1 + floor($2 * power(random(), 3))::bigint AS t \
                    FROM generate_series(1, 5 + (g % 20))) s), \
                1, now() - make_interval(secs => ($1 - g) * 30), \
                now() - make_interval(secs => random() * 86400 * 365), \
                CASE WHEN g % 50 = 0 THEN now() END, \
                CASE WHEN g % 100 = 0 THEN 0 \
                     ELSE ('x' || substr(md5(g::text), 1, 16))::bit(64)::bigint END \
         FROM generate_series(1, $1) g",
    )
    .bind(posts)
    .bind(TAGS as f64)
    .execute(db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE tags t SET post_count = s.c FROM \
           (SELECT unnest(tag_ids) AS id, count(*)::int AS c FROM posts \
            WHERE deleted_at IS NULL GROUP BY 1) s \
         WHERE t.id = s.id",
    )
    .execute(db)
    .await
    .unwrap();
    // Recent votes, for Explore's trending section.
    sqlx::query(
        "INSERT INTO post_votes (post_id, user_id, value, voted_at) \
         SELECT id, 1, 1, now() - make_interval(secs => random() * 86400 * 14) \
         FROM posts WHERE id % 20 = 0",
    )
    .execute(db)
    .await
    .unwrap();
    sqlx::query("VACUUM ANALYZE").execute(db).await.unwrap();
    println!("seeded {posts} posts in {:.1?}", seeding.elapsed());

    let mid = posts / 2;
    let calls = [
        "/api/v1/posts".to_owned(),
        "/api/v1/posts?page=50".into(),
        "/api/v1/posts?page=1000".into(),
        "/api/v1/posts?q=tag_1".into(),
        "/api/v1/posts?q=tag_4000".into(),
        "/api/v1/posts?q=tag_1+tag_2+tag_3".into(),
        "/api/v1/posts?q=tag_2+-tag_1".into(),
        "/api/v1/posts?q=tag_300+tag_301".into(),
        "/api/v1/posts?q=rating:e+type:video".into(),
        "/api/v1/posts?q=order:score".into(),
        "/api/v1/posts?q=order:size_asc".into(),
        "/api/v1/posts?q=order:pixels".into(),
        "/api/v1/posts?q=order:tagcount".into(),
        "/api/v1/posts?q=order:updated".into(),
        "/api/v1/posts?q=order:random".into(),
        "/api/v1/posts?q=tag_1+order:score".into(),
        "/api/v1/posts?q=tag_50+order:pixels".into(),
        "/api/v1/posts?q=status:deleted".into(),
        "/api/v1/posts?q=order:longside".into(),
        "/api/v1/posts?q=date:%3E30d".into(),
        "/api/v1/posts?q=date:%3C7d".into(),
        format!(
            "/api/v1/posts?q=date:{}",
            chrono::Utc::now().format("%Y-%m")
        ),
        "/api/v1/posts?q=width:%3E3900".into(),
        "/api/v1/posts?q=mpixels:%3E12".into(),
        "/api/v1/posts?q=ratio:16:9".into(),
        "/api/v1/posts?q=tag_1+width:%3E1920".into(),
        "/api/v1/posts?q=tag_300+date:%3E30d".into(),
        "/api/v1/posts?q=-width:%3E1000".into(),
        format!("/api/v1/posts/{mid}"),
        format!("/api/v1/posts/{mid}/neighbors"),
        format!("/api/v1/posts/{mid}/neighbors?q=tag_1+order:score"),
        format!("/api/v1/posts/{mid}/related"),
        format!("/api/v1/posts/{}/related", mid + 1),
        "/api/v1/tags?q=tag_12".into(),
        "/api/v1/tags?order=count".into(),
        "/api/v1/explore".into(),
        "/api/v1/explore?q=tag_1".into(),
        "/api/v1/explore?q=tag_4000".into(),
        "/api/v1/admin/stats".into(),
    ];
    let mut slow = Vec::new();
    println!("{:<52} {:>9} {:>9}", "call", "median", "max");
    for path in &calls {
        let mut times = Vec::with_capacity(RUNS);
        for run in 0..=RUNS {
            let start = Instant::now();
            let r = app.call(Method::GET, path, Some(&token), None).await;
            let took = start.elapsed();
            assert_eq!(r.status, StatusCode::OK, "{path}: {:?}", r.json());
            if run > 0 {
                times.push(took);
            }
        }
        let max = times.iter().max().copied().unwrap_or_default();
        let med = median(times);
        println!("{path:<52} {med:>9.1?} {max:>9.1?}");
        if med > LIMIT {
            slow.push(json!({ "call": path, "median_ms": med.as_millis() }));
        }
    }
    if std::env::var_os("STARBOORU_BENCH_KEEP").is_some() {
        println!("kept database {}", app.db_name());
    } else {
        app.finish().await;
    }
    assert!(slow.is_empty(), "slower than {LIMIT:?}: {slow:?}");
}
