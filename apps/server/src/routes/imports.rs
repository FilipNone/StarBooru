//! Background URL imports: the server downloads each address on its own, so an
//! import goes on when the app that asked for it is closed.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use starbooru_core::tag::normalize_tags;
use starbooru_core::{ImportBatch, ImportJob, ImportRequest, JobState, MAX_IMPORT_BATCH, Rating};
use tracing::{info, warn};

use super::posts::{clean_source, import_one};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Imports running at once, server-wide.
const WORKERS: usize = 2;
/// Most imports one account may have waiting or running.
const MAX_WAITING: i64 = 2000;
/// Longest a worker sleeps before looking for work it wasn't told about.
const IDLE: Duration = Duration::from_secs(60);
/// How many times an import waits for a site that says it is busy.
const MAX_ATTEMPTS: i16 = 6;
/// The wait before the second try; each later one is twice as long.
const FIRST_WAIT_SECS: u64 = 15;
/// The longest an import waits between two tries.
const MAX_WAIT_SECS: u64 = 600;

/// How long an import waits before trying a busy site again: what the site
/// asked for, and longer each time.
fn retry_wait(attempts: i16, asked: u64) -> u64 {
    let step = u32::try_from(attempts.max(0)).unwrap_or(0).min(16);
    let backoff = FIRST_WAIT_SECS.saturating_mul(1u64 << step);
    asked.max(backoff).min(MAX_WAIT_SECS)
}
/// Finished imports are listed for a day and deleted after a week.
const LISTED_HOURS: i32 = 24;
const KEPT_DAYS: i32 = 7;

#[derive(FromRow)]
struct Row {
    id: i64,
    url: String,
    source: Option<String>,
    tags: Vec<String>,
    rating: i16,
    state: String,
    post_id: Option<i64>,
    error: Option<String>,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

impl From<Row> for ImportJob {
    fn from(r: Row) -> Self {
        ImportJob {
            id: r.id,
            url: r.url,
            source: r.source,
            tags: r.tags,
            rating: Rating::from_db(r.rating).unwrap_or_default(),
            state: JobState::parse(&r.state).unwrap_or(JobState::Failed),
            post_id: r.post_id,
            error: r.error,
            created_at: r.created_at,
            finished_at: r.finished_at,
            progress: None,
        }
    }
}

/// `POST /imports`: queues addresses for the server to download.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(batch): Json<ImportBatch>,
) -> AppResult<Json<Vec<ImportJob>>> {
    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("URL import is turned off on this server".into()))?;
    if batch.items.is_empty() || batch.items.len() > MAX_IMPORT_BATCH {
        return Err(AppError::BadRequest(format!(
            "send between 1 and {MAX_IMPORT_BATCH} addresses"
        )));
    }
    let mut items = Vec::with_capacity(batch.items.len());
    for req in batch.items {
        let url = req.url.trim().to_owned();
        fetcher
            .check(&url)
            .map_err(|e| AppError::BadRequest(format!("{url}: {e}")))?;
        let tags = normalize_tags(req.tags.iter().map(String::as_str))?;
        let source = clean_source(req.source.unwrap_or_else(|| url.clone()))?;
        items.push((url, source, tags, req.rating));
    }

    let mut tx = state.db.begin().await?;
    // Serialises one account's batches, so the limit holds.
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let waiting: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM import_jobs WHERE user_id = $1 AND state IN ('queued', 'running')",
    )
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let adding = i64::try_from(items.len()).unwrap_or(i64::MAX);
    if waiting.saturating_add(adding) > MAX_WAITING {
        return Err(AppError::Conflict {
            code: "too_many_imports",
            message: format!(
                "you have {waiting} imports waiting; at most {MAX_WAITING} can wait at once"
            ),
        });
    }
    let mut jobs = Vec::with_capacity(items.len());
    for (url, source, tags, rating) in items {
        let row: Row = sqlx::query_as(
            "INSERT INTO import_jobs (user_id, url, source, tags, rating) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id, url, source, tags, rating, state, \
             post_id, error, created_at, finished_at",
        )
        .bind(user.id)
        .bind(url)
        .bind(source)
        .bind(tags)
        .bind(rating.to_db())
        .fetch_one(&mut *tx)
        .await?;
        jobs.push(ImportJob::from(row));
    }
    tx.commit().await?;
    state.imports.notify_waiters();
    Ok(Json(jobs))
}

/// `GET /imports`: the caller's imports still waiting or running, and those
/// finished in the last day, newest first.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<ImportJob>>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, url, source, tags, rating, state, post_id, error, created_at, finished_at \
         FROM import_jobs WHERE user_id = $1 \
         AND (state IN ('queued', 'running') OR finished_at > now() - make_interval(hours => $2)) \
         ORDER BY id DESC LIMIT $3",
    )
    .bind(user.id)
    .bind(LISTED_HOURS)
    .bind(MAX_WAITING.saturating_add(500))
    .fetch_all(&state.db)
    .await?;
    let mut jobs: Vec<ImportJob> = rows.into_iter().map(ImportJob::from).collect();
    for job in jobs.iter_mut().filter(|job| job.state == JobState::Running) {
        job.progress = state.transfers.get(job.id);
    }
    Ok(Json(jobs))
}

/// `DELETE /imports/{id}`: cancels a waiting import or hides a finished one.
/// 409 `running` while it is downloading.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let found: Option<String> =
        sqlx::query_scalar("SELECT state FROM import_jobs WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;
    match found.as_deref() {
        None => Err(AppError::NotFound),
        Some("running") => Err(AppError::Conflict {
            code: "running",
            message: "this import is downloading; it can't be stopped now".into(),
        }),
        Some(_) => {
            // `state <> 'running'` again: a worker may have claimed it meanwhile
            let gone = sqlx::query(
                "DELETE FROM import_jobs WHERE id = $1 AND user_id = $2 AND state <> 'running'",
            )
            .bind(id)
            .bind(user.id)
            .execute(&state.db)
            .await?;
            if gone.rows_affected() == 0 {
                Err(AppError::Conflict {
                    code: "running",
                    message: "this import is downloading; it can't be stopped now".into(),
                })
            } else {
                Ok(StatusCode::NO_CONTENT)
            }
        }
    }
}

/// `DELETE /imports`: hides the caller's finished imports.
pub async fn clear(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    sqlx::query(
        "DELETE FROM import_jobs WHERE user_id = $1 AND state NOT IN ('queued', 'running')",
    )
    .bind(user.id)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// At startup: requeues imports a restart interrupted, then runs the workers
/// and deletes old finished imports once an hour.
pub async fn run_imports(state: AppState) {
    if state.fetcher.is_none() {
        return;
    }
    match requeue_interrupted(&state).await {
        Ok(0) => {}
        Ok(n) => info!("imports: requeued {n} interrupted by a restart"),
        Err(e) => warn!("imports: cannot requeue interrupted imports: {e}"),
    }
    for _ in 0..WORKERS {
        tokio::spawn(worker(state.clone()));
    }
    let mut tick = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tick.tick().await;
        let old = sqlx::query(
            "DELETE FROM import_jobs WHERE state NOT IN ('queued', 'running') \
             AND finished_at < now() - make_interval(days => $1)",
        )
        .bind(KEPT_DAYS)
        .execute(&state.db)
        .await;
        if let Err(e) = old {
            warn!("imports: cannot delete old imports: {e}");
        }
    }
}

/// Imports left `running` when the server stopped go back in the queue.
async fn requeue_interrupted(state: &AppState) -> AppResult<u64> {
    let r = sqlx::query("UPDATE import_jobs SET state = 'queued' WHERE state = 'running'")
        .execute(&state.db)
        .await?;
    Ok(r.rows_affected())
}

async fn worker(state: AppState) {
    loop {
        // Registered before looking, so a batch queued meanwhile still wakes us.
        let woken = state.imports.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();
        match run_next(&state).await {
            Ok(true) => continue,
            Ok(false) => {
                // Until told of new work, or until a waiting import is due.
                let nap = next_due(&state).await.map_or(IDLE, |due| due.min(IDLE));
                let _ = tokio::time::timeout(nap, woken).await;
            }
            Err(e) => {
                warn!("imports: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// How long until the first import that is waiting for a busy site may try
/// again. `None` when none is.
async fn next_due(state: &AppState) -> Option<Duration> {
    let due: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT min(not_before) FROM import_jobs WHERE state = 'queued' AND not_before IS NOT NULL",
    )
    .fetch_one(&state.db)
    .await
    .ok()?;
    let wait = (due? - Utc::now()).to_std().unwrap_or_default();
    Some(wait + Duration::from_millis(50))
}

/// Claims and runs the oldest waiting import that is due. `false` when none
/// is.
pub(crate) async fn run_next(state: &AppState) -> AppResult<bool> {
    #[derive(FromRow)]
    struct Claimed {
        id: i64,
        user_id: i64,
        url: String,
        source: Option<String>,
        tags: Vec<String>,
        rating: i16,
        attempts: i16,
        active: bool,
    }
    let claimed: Option<Claimed> = sqlx::query_as(
        "UPDATE import_jobs j SET state = 'running' FROM users u \
         WHERE j.id = (SELECT id FROM import_jobs WHERE state = 'queued' \
                       AND (not_before IS NULL OR not_before <= now()) \
                       ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED) \
         AND u.id = j.user_id \
         RETURNING j.id, j.user_id, j.url, j.source, j.tags, j.rating, j.attempts, \
                   u.disabled_at IS NULL AS active",
    )
    .fetch_optional(&state.db)
    .await?;
    let Some(job) = claimed else {
        return Ok(false);
    };
    let outcome = if job.active {
        let req = ImportRequest {
            url: job.url,
            tags: job.tags,
            rating: Rating::from_db(job.rating).unwrap_or_default(),
            source: job.source,
        };
        import_one(state, job.user_id, req, Some(job.id)).await
    } else {
        Err(AppError::Denied("this account is disabled"))
    };
    state.transfers.end(job.id);
    let (done, post_id, error) = match outcome {
        Ok(post) => (JobState::Done, Some(post.id), None),
        // The site is busy: back in the queue, to try again later.
        Err(AppError::TooManyRequests { retry_after }) if job.attempts < MAX_ATTEMPTS => {
            let wait = retry_wait(job.attempts, retry_after);
            sqlx::query(
                "UPDATE import_jobs SET state = 'queued', attempts = attempts + 1, error = $2, \
                 not_before = now() + $3::bigint * interval '1 second' WHERE id = $1",
            )
            .bind(job.id)
            .bind("that site is busy; this import will try again shortly")
            .bind(i64::try_from(wait).unwrap_or(i64::MAX))
            .execute(&state.db)
            .await?;
            return Ok(true);
        }
        Err(AppError::TooManyRequests { .. }) => (
            JobState::Failed,
            None,
            Some("that site kept saying it is busy; try again later".to_owned()),
        ),
        Err(AppError::Conflict {
            code: "duplicate",
            message,
        }) => (JobState::Duplicate, None, Some(message)),
        Err(AppError::Internal(detail)) => {
            warn!(job = job.id, "import failed: {detail}");
            (
                JobState::Failed,
                None,
                Some("internal server error".to_owned()),
            )
        }
        Err(e) => (JobState::Failed, None, Some(e.to_string())),
    };
    sqlx::query(
        "UPDATE import_jobs SET state = $2, post_id = $3, error = $4, finished_at = now() \
         WHERE id = $1",
    )
    .bind(job.id)
    .bind(done.as_str())
    .bind(post_id)
    .bind(error)
    .execute(&state.db)
    .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use serde_json::{Value, json};

    use super::*;
    use crate::testing::{self, TestApp};

    /// A site with one image, a redirect to it and a missing page.
    async fn site() -> String {
        use axum::response::Redirect;
        use axum::routing::get;
        let png = testing::png(16, 12, 11);
        let app = axum::Router::new()
            .route("/a.png", get(move || async move { png.clone() }))
            .route("/hop", get(|| async { Redirect::temporary("/a.png") }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    async fn app() -> Option<TestApp> {
        testing::app_with(|c| {
            c.url_import = Some(crate::fetch::Reach::Any);
            c.max_upload_bytes = 1024 * 1024;
        })
        .await
    }

    async fn queue(app: &TestApp, token: &str, items: Value) -> testing::Resp {
        app.call(
            Method::POST,
            "/api/v1/imports",
            Some(token),
            Some(json!({ "items": items })),
        )
        .await
    }

    async fn drain(app: &TestApp) -> usize {
        let mut ran = 0;
        while run_next(&app.state).await.unwrap() {
            ran += 1;
        }
        ran
    }

    #[tokio::test]
    async fn imports_run_in_the_background() {
        let Some(app) = app().await else {
            return;
        };
        let t = app.register("admin").await;
        let base = site().await;
        let r = queue(
            &app,
            &t,
            json!([
                { "url": format!("{base}/hop"), "tags": ["Blue Sky"], "rating": "sensitive", "source": "https://example.com/post/1" },
                { "url": format!("{base}/a.png") },
                { "url": format!("{base}/missing") },
            ]),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let queued = r.json();
        assert_eq!(queued.as_array().unwrap().len(), 3);
        assert!(
            queued
                .as_array()
                .unwrap()
                .iter()
                .all(|j| j["state"] == "queued")
        );
        assert_eq!(queued[0]["tags"], json!(["blue_sky"]));

        assert_eq!(drain(&app).await, 3);
        let jobs = app.get("/api/v1/imports", &t).await.json();
        // newest first
        let by_url = |end: &str| {
            jobs.as_array()
                .unwrap()
                .iter()
                .find(|j| j["url"].as_str().unwrap().ends_with(end))
                .unwrap()
                .clone()
        };
        let done = by_url("/hop");
        assert_eq!(done["state"], "done");
        assert!(done["finished_at"].is_string());
        let post = app
            .get(&format!("/api/v1/posts/{}", done["post_id"]), &t)
            .await
            .json();
        assert_eq!(post["tags"], json!(["blue_sky"]));
        assert_eq!(post["rating"], "sensitive");
        assert_eq!(post["source"], "https://example.com/post/1");

        let dup = by_url("/a.png");
        assert_eq!(dup["state"], "duplicate");
        assert!(dup["error"].as_str().unwrap().contains("already"));
        let failed = by_url("/missing");
        assert_eq!(failed["state"], "failed");
        assert!(failed["error"].as_str().unwrap().contains("404"));
        app.finish().await;
    }

    #[tokio::test]
    async fn a_running_import_says_how_far_it_is() {
        let Some(app) = app().await else {
            return;
        };
        let t = app.register("admin").await;
        let base = site().await;
        let jobs = queue(
            &app,
            &t,
            json!([{ "url": format!("{base}/a.png") }, { "url": format!("{base}/hop") }]),
        )
        .await
        .json();
        let id = jobs[0]["id"].as_i64().unwrap();
        let waiting = jobs[1]["id"].as_i64().unwrap();
        assert!(jobs[0].get("progress").is_none());

        // As a worker halfway through the first would have it.
        sqlx::query("UPDATE import_jobs SET state = 'running' WHERE id = $1")
            .bind(id)
            .execute(app.db())
            .await
            .unwrap();
        app.state.transfers.seen(id, 50, Some(200), Utc::now());
        // Left over from somewhere: a job that isn't running shows nothing.
        app.state.transfers.seen(waiting, 1, None, Utc::now());
        let list = app.get("/api/v1/imports", &t).await.json();
        let job = |id: i64| {
            list.as_array()
                .unwrap()
                .iter()
                .find(|j| j["id"] == id)
                .unwrap()
                .clone()
        };
        assert_eq!(
            job(id)["progress"],
            json!({"done": 50, "total": 200, "speed": 0})
        );
        assert!(job(waiting).get("progress").is_none());

        // Run for real, the download is followed and then forgotten.
        assert_eq!(requeue_interrupted(&app.state).await.unwrap(), 1);
        assert_eq!(drain(&app).await, 2);
        assert_eq!(app.state.transfers.get(id), None);
        assert_eq!(app.state.transfers.get(waiting), None);
        let list = app.get("/api/v1/imports", &t).await.json();
        assert!(
            list.as_array()
                .unwrap()
                .iter()
                .all(|j| j.get("progress").is_none())
        );
        app.finish().await;
    }

    #[test]
    fn waits_longer_each_time_and_as_long_as_asked() {
        assert_eq!(retry_wait(0, 0), 15);
        assert_eq!(retry_wait(1, 0), 30);
        assert_eq!(retry_wait(3, 0), 120);
        assert_eq!(retry_wait(0, 42), 42);
        assert_eq!(retry_wait(3, 42), 120);
        assert_eq!(retry_wait(i16::MAX, u64::MAX), MAX_WAIT_SECS);
        assert_eq!(retry_wait(-1, 0), 15);
    }

    #[tokio::test]
    async fn an_import_waits_for_a_busy_site_and_tries_again() {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::routing::get;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let Some(app) = app().await else {
            return;
        };
        let t = app.register("admin").await;
        // Busy the first time it is asked, and whenever `/never` is.
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        let png = testing::png(20, 10, 4);
        let site = axum::Router::new()
            .route(
                "/a.png",
                get(move || {
                    let first = count.fetch_add(1, Ordering::SeqCst) == 0;
                    let png = png.clone();
                    async move {
                        if first {
                            StatusCode::TOO_MANY_REQUESTS.into_response()
                        } else {
                            png.into_response()
                        }
                    }
                }),
            )
            .route("/never", get(|| async { StatusCode::SERVICE_UNAVAILABLE }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, site).await });

        let jobs = queue(&app, &t, json!([{ "url": format!("{base}/a.png") }]))
            .await
            .json();
        let id = jobs[0]["id"].as_i64().unwrap();
        assert!(run_next(&app.state).await.unwrap());
        let job = app.get("/api/v1/imports", &t).await.json()[0].clone();
        assert_eq!(job["state"], "queued");
        assert!(job["error"].as_str().unwrap().contains("busy"));
        // Not due yet: nothing to run, and a worker knows how long to sleep.
        assert!(!run_next(&app.state).await.unwrap());
        let due = next_due(&app.state).await.unwrap();
        assert!((10..=16).contains(&due.as_secs()), "{due:?}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        // Once the wait is over, it goes through.
        let overdue = "UPDATE import_jobs SET not_before = now() - interval '1 second'";
        sqlx::query(overdue).execute(app.db()).await.unwrap();
        app.state.pace.forget();
        assert!(run_next(&app.state).await.unwrap());
        let job = app.get("/api/v1/imports", &t).await.json()[0].clone();
        assert_eq!(
            (job["id"].as_i64(), job["state"].as_str()),
            (Some(id), Some("done"))
        );
        assert!(job["error"].is_null());
        assert_eq!(next_due(&app.state).await, None);

        // A site that never lets up fails the import in the end.
        queue(&app, &t, json!([{ "url": format!("{base}/never") }])).await;
        let mut tries = 0;
        loop {
            sqlx::query(overdue).execute(app.db()).await.unwrap();
            app.state.pace.forget();
            if !run_next(&app.state).await.unwrap() {
                break;
            }
            tries += 1;
            assert!(tries < 20, "it never gave up");
        }
        assert_eq!(tries, usize::try_from(MAX_ATTEMPTS).unwrap() + 1);
        let job = app.get("/api/v1/imports", &t).await.json()[0].clone();
        assert_eq!(job["state"], "failed");
        assert!(job["error"].as_str().unwrap().contains("kept saying"));
        app.finish().await;
    }

    #[tokio::test]
    async fn refuses_bad_batches() {
        let Some(app) = app().await else {
            return;
        };
        let t = app.register("admin").await;
        assert_eq!(
            queue(&app, &t, json!([])).await.status,
            StatusCode::BAD_REQUEST
        );
        let r = queue(&app, &t, json!([{ "url": "ftp://example.com/a.png" }])).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = queue(
            &app,
            &t,
            json!([{ "url": "https://example.com/a.png", "tags": ["-nope"] }]),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let many: Vec<Value> = (0..=MAX_IMPORT_BATCH)
            .map(|i| json!({ "url": format!("https://example.com/{i}.png") }))
            .collect();
        assert_eq!(
            queue(&app, &t, json!(many)).await.status,
            StatusCode::BAD_REQUEST
        );
        // nothing was queued by the refused batches
        assert_eq!(app.get("/api/v1/imports", &t).await.json(), json!([]));
        app.finish().await;

        // off by default for private addresses, and entirely when URL import is off
        let Some(app) = testing::app().await else {
            return;
        };
        let t = app.register("admin").await;
        let r = queue(&app, &t, json!([{ "url": "http://127.0.0.1:9/a.png" }])).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        assert!(
            r.json()["message"]
                .as_str()
                .unwrap()
                .contains("private network")
        );
        app.finish().await;
        let Some(app) = testing::app_with(|c| c.url_import = None).await else {
            return;
        };
        let t = app.register("admin").await;
        let r = queue(&app, &t, json!([{ "url": "https://example.com/a.png" }])).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        app.finish().await;
    }

    #[tokio::test]
    async fn cancel_clear_and_restart() {
        let Some(app) = app().await else {
            return;
        };
        let t = app.register("admin").await;
        let other = app.register("other").await;
        let base = site().await;
        let jobs = queue(
            &app,
            &t,
            json!([
                { "url": format!("{base}/a.png") },
                { "url": format!("{base}/missing") },
                { "url": format!("{base}/hop") },
            ]),
        )
        .await
        .json();
        let id = |i: usize| jobs[i]["id"].as_i64().unwrap();
        let path = |i: usize| format!("/api/v1/imports/{}", id(i));

        // someone else's import isn't theirs to cancel
        let r = app.call(Method::DELETE, &path(2), Some(&other), None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        // a waiting import can be cancelled
        let r = app.call(Method::DELETE, &path(2), Some(&t), None).await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);

        // a running one can't; a restart puts it back in the queue
        sqlx::query("UPDATE import_jobs SET state = 'running' WHERE id = $1")
            .bind(id(0))
            .execute(app.db())
            .await
            .unwrap();
        let r = app.call(Method::DELETE, &path(0), Some(&t), None).await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["code"], "running");
        assert_eq!(requeue_interrupted(&app.state).await.unwrap(), 1);

        assert_eq!(drain(&app).await, 2);
        let list = app.get("/api/v1/imports", &t).await.json();
        assert_eq!(list.as_array().unwrap().len(), 2);
        assert!(app.get("/api/v1/imports", &other).await.json() == json!([]));

        // clearing hides finished imports and keeps waiting ones
        queue(&app, &t, json!([{ "url": format!("{base}/hop") }])).await;
        let r = app
            .call(Method::DELETE, "/api/v1/imports", Some(&t), None)
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        let list = app.get("/api/v1/imports", &t).await.json();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["state"], "queued");
        app.finish().await;
    }

    #[tokio::test]
    async fn a_disabled_account_imports_nothing() {
        let Some(app) = app().await else {
            return;
        };
        let _admin = app.register("admin").await;
        let t = app.register("someone").await;
        let base = site().await;
        queue(&app, &t, json!([{ "url": format!("{base}/a.png") }])).await;
        sqlx::query("UPDATE users SET disabled_at = now() WHERE username = 'someone'")
            .execute(app.db())
            .await
            .unwrap();
        assert_eq!(drain(&app).await, 1);
        let state: (String, Option<String>, i64) =
            sqlx::query_as("SELECT state, error, (SELECT count(*) FROM posts) FROM import_jobs")
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(state.0, "failed");
        assert!(state.1.unwrap().contains("disabled"));
        assert_eq!(state.2, 0);
        app.finish().await;
    }
}
