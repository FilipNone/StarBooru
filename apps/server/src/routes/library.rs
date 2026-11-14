//! Importing a whole library from another booru (`/library-imports`). The server
//! walks the other site on its own, one import at a time, so it goes on with the
//! app closed and picks up where it was after a restart. What each system's
//! answers mean is in `crate::sources`.

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, FromRow};
use starbooru_core::{
    LibraryCheck, LibraryFailure, LibraryImport, LibraryPhase, LibraryRetry, LibrarySite,
    LibrarySource, LibraryState, MAX_ACTIVE_LIBRARY_IMPORTS, Rating, Role, clean_credentials,
};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::posts::{Stored, import_vote, merge_into, store_post};
use super::tags::import_rules;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::fetch::{FetchError, Fetcher, SiteCall};
use crate::sources::{Answer, Call, Group, Item, MAX_ANSWER, Site};
use crate::state::AppState;
use crate::uploads::PostFields;

/// Longest the worker sleeps before looking for work it wasn't told about.
const IDLE: Duration = Duration::from_secs(60);
/// Rate limits count over a minute or so, more than the waits below add up to.
/// LANBooru's default is 120 requests a minute, so a large library hits it often.
const RATE_LIMIT_WAITS: u32 = 8;
const RATE_LIMIT_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(1)
} else {
    Duration::from_secs(15)
};
/// Tries per request while a site is struggling, and the wait before the second.
const ATTEMPTS: u32 = 5;
const RETRY_BASE: Duration = if cfg!(test) {
    Duration::from_millis(5)
} else {
    Duration::from_secs(1)
};
/// Files downloading at once: the next is ready when one has been stored.
const DOWNLOADS: usize = 2;
/// Downloads in a row that may fail on the network before the import gives up.
const MAX_LOST: u32 = 50;
/// Failures listed for an import, and imports listed for an account.
const MAX_FAILURES_LISTED: i64 = 500;
const MAX_LISTED: i64 = 50;

const COLUMNS: &str = "id, source, base, state, phase, total, imported, merged, failed, rules, \
                       note, error, created_at, finished_at";

#[derive(FromRow)]
struct Row {
    id: i64,
    source: String,
    base: String,
    state: String,
    phase: String,
    total: Option<i64>,
    imported: i64,
    merged: i64,
    failed: i64,
    rules: i64,
    note: Option<String>,
    error: Option<String>,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

fn count(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

impl TryFrom<Row> for LibraryImport {
    type Error = AppError;

    fn try_from(r: Row) -> AppResult<Self> {
        Ok(LibraryImport {
            id: r.id,
            source: LibrarySource::parse(&r.source)
                .ok_or_else(|| AppError::internal("unknown library source in the database"))?,
            base: r.base,
            state: LibraryState::parse(&r.state).unwrap_or(LibraryState::Failed),
            phase: LibraryPhase::parse(&r.phase).unwrap_or(LibraryPhase::Posts),
            total: r.total.map(count),
            imported: count(r.imported),
            merged: count(r.merged),
            failed: count(r.failed),
            rules: count(r.rules),
            note: r.note,
            error: r.error,
            created_at: r.created_at,
            finished_at: r.finished_at,
        })
    }
}

/// The site a request names, with what's wrong with it in the user's terms.
fn site_of(
    source: LibrarySource,
    base: &str,
    login: Option<String>,
    secret: Option<String>,
) -> AppResult<Site> {
    Site::new(source, base, login, secret).ok_or_else(|| {
        AppError::BadRequest("that address or key has characters that can't be sent".into())
    })
}

/// The client for what this account may reach: an administrator may read a site
/// on the server's own network, others only as far as URL import allows.
fn fetcher_for(state: &AppState, admin: bool) -> AppResult<&Fetcher> {
    let fetcher = if admin {
        state.trusted_fetcher.as_ref()
    } else {
        state.fetcher.as_ref()
    };
    fetcher.ok_or_else(|| AppError::BadRequest("imports are turned off on this server".into()))
}

/// A failed request in terms that say what to do about it.
fn explain(source: LibrarySource, e: &FetchError) -> String {
    match e {
        FetchError::Status(401 | 403) => format!(
            "{} refused this account: check the user name and key",
            source.name()
        ),
        FetchError::Status(404) => not_that_system(source),
        other => other.to_string(),
    }
}

fn not_that_system(source: LibrarySource) -> String {
    match source {
        LibrarySource::Shimmie2 => "this address doesn't answer like Shimmie2 with its Ouroboros \
                                    API extension: turn that on in the Extension Manager"
            .to_owned(),
        other => format!(
            "this address doesn't answer like {}: use the site's main address, not a page on it",
            other.name()
        ),
    }
}

async fn ask(fetcher: &Fetcher, site: &Site, call: &Call) -> Result<Vec<u8>, FetchError> {
    fetcher
        .call(
            SiteCall {
                url: call.url.clone(),
                body: call.body.as_deref(),
                site: &site.base,
                headers: site.headers(),
                json: true,
            },
            MAX_ANSWER,
        )
        .await
}

async fn send(fetcher: &Fetcher, site: &Site, call: &Call) -> Result<Vec<u8>, FetchError> {
    retrying(|| ask(fetcher, site, call)).await
}

/// Whether trying the same request again may help.
fn passing(e: &FetchError) -> bool {
    matches!(
        e,
        FetchError::Status(429 | 500 | 502 | 503 | 504)
            | FetchError::Timeout
            | FetchError::Unreachable
    )
}

async fn retrying<F, Fut>(mut attempt: F) -> Result<Vec<u8>, FetchError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Vec<u8>, FetchError>>,
{
    let mut wait = RETRY_BASE;
    let mut tries = 1;
    let mut limited = 0;
    loop {
        match attempt().await {
            Err(FetchError::Status(429)) if limited < RATE_LIMIT_WAITS => {
                limited += 1;
                tokio::time::sleep(RATE_LIMIT_WAIT).await;
            }
            Err(e) if passing(&e) && tries < ATTEMPTS => {
                tries += 1;
                tokio::time::sleep(wait).await;
                wait = wait.saturating_mul(2);
            }
            other => return other,
        }
    }
}

/// `POST /library-imports/check`: reads the first page, so a wrong address or key
/// shows before anything starts. Someone is waiting on it, so it asks only once.
pub async fn check(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<LibrarySite>,
) -> AppResult<Json<LibraryCheck>> {
    let req = req
        .clean()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let admin = user.role == Role::Admin;
    let fetcher = fetcher_for(&state, admin)?;
    let site = site_of(req.source, &req.base, req.login, req.secret)?;
    let refused = |e: FetchError| match e {
        FetchError::BadUrl | FetchError::Blocked => AppError::BadRequest(e.to_string()),
        other => AppError::Upstream(explain(req.source, &other)),
    };
    let call = site
        .call(LibraryPhase::Posts, "")
        .ok_or_else(|| AppError::internal("a source without posts"))?;
    let body = ask(fetcher, &site, &call).await.map_err(refused)?;
    let answer = site
        .parse(LibraryPhase::Posts, "", &body)
        .ok_or_else(|| AppError::Upstream(not_that_system(req.source)))?;
    let total = match (answer.total, site.count_call()) {
        (Some(total), _) => Some(total),
        (None, Some(call)) => ask(fetcher, &site, &call)
            .await
            .ok()
            .and_then(|body| site.parse_count(&body)),
        (None, None) => None,
    };
    Ok(Json(LibraryCheck {
        total,
        rules: admin,
    }))
}

/// Refuses another import past the limit. Locks the account row, so two requests
/// can't both slip under it.
async fn check_room(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, user_id: i64) -> AppResult<()> {
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM library_imports WHERE user_id = $1 AND state IN ('queued', 'running')",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    if active >= MAX_ACTIVE_LIBRARY_IMPORTS {
        return Err(AppError::Denied(
            "too many library imports are waiting; let one finish first",
        ));
    }
    Ok(())
}

/// `POST /library-imports`
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<LibrarySite>,
) -> AppResult<(StatusCode, Json<LibraryImport>)> {
    fetcher_for(&state, user.role == Role::Admin)?;
    let req = req
        .clean()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    site_of(req.source, &req.base, req.login.clone(), req.secret.clone())?;
    let mut tx = state.db.begin().await?;
    check_room(&mut tx, user.id).await?;
    let row: Row = sqlx::query_as(AssertSqlSafe(format!(
        "INSERT INTO library_imports (user_id, source, base, login, secret, unrated) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
    )))
    .bind(user.id)
    .bind(req.source.as_str())
    .bind(&req.base)
    .bind(&req.login)
    .bind(&req.secret)
    .bind(req.unrated.to_db())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    state.library_imports.notify_one();
    Ok((StatusCode::CREATED, Json(row.try_into()?)))
}

/// `GET /library-imports`: this account's, newest first.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<LibraryImport>>> {
    let rows: Vec<Row> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM library_imports WHERE user_id = $1 ORDER BY id DESC LIMIT $2"
    )))
    .bind(user.id)
    .bind(MAX_LISTED)
    .fetch_all(&state.db)
    .await?;
    let imports: AppResult<Vec<LibraryImport>> = rows.into_iter().map(TryInto::try_into).collect();
    Ok(Json(imports?))
}

async fn owned(state: &AppState, user_id: i64, id: i64) -> AppResult<LibraryImport> {
    let row: Row = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM library_imports WHERE id = $1 AND user_id = $2"
    )))
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;
    row.try_into()
}

/// `POST /library-imports/{id}/stop`: what came across stays.
pub async fn stop(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<LibraryImport>> {
    owned(&state, user.id, id).await?;
    sqlx::query(
        "UPDATE library_imports SET state = 'stopped', login = NULL, secret = NULL, \
         finished_at = now() WHERE id = $1 AND state IN ('queued', 'running')",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(Json(owned(&state, user.id, id).await?))
}

/// `POST /library-imports/{id}/retry`: walks the site again, skipping what already
/// came across, so it tries the failed posts and finds any new ones.
pub async fn retry(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<LibraryRetry>,
) -> AppResult<Json<LibraryImport>> {
    fetcher_for(&state, user.role == Role::Admin)?;
    let import = owned(&state, user.id, id).await?;
    if import.state.is_active() {
        return Err(AppError::Conflict {
            code: "running",
            message: "this import is still going".into(),
        });
    }
    let (login, secret) = clean_credentials(import.source, req.login, req.secret)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    site_of(import.source, &import.base, login.clone(), secret.clone())?;
    let mut tx = state.db.begin().await?;
    check_room(&mut tx, user.id).await?;
    sqlx::query("DELETE FROM library_import_items WHERE import_id = $1 AND error IS NOT NULL")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let requeued = sqlx::query(
        "UPDATE library_imports SET state = 'queued', login = $2, secret = $3, \
         phase = CASE WHEN phase = 'rules' THEN 'rules' ELSE 'posts' END, position = '', \
         failed = 0, note = NULL, error = NULL, finished_at = NULL \
         WHERE id = $1 AND state NOT IN ('queued', 'running')",
    )
    .bind(id)
    .bind(&login)
    .bind(&secret)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if requeued == 0 {
        return Err(AppError::Conflict {
            code: "running",
            message: "this import is still going".into(),
        });
    }
    tx.commit().await?;
    state.library_imports.notify_one();
    Ok(Json(owned(&state, user.id, id).await?))
}

/// `DELETE /library-imports/{id}`: forgets the import; its posts stay.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    owned(&state, user.id, id).await?;
    let removed = sqlx::query(
        "DELETE FROM library_imports WHERE id = $1 AND state NOT IN ('queued', 'running')",
    )
    .bind(id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if removed == 0 {
        return Err(AppError::Conflict {
            code: "running",
            message: "stop this import before removing it".into(),
        });
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /library-imports/{id}/failures`
pub async fn failures(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<LibraryFailure>>> {
    owned(&state, user.id, id).await?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT remote_id, error FROM library_import_items \
         WHERE import_id = $1 AND error IS NOT NULL ORDER BY remote_id LIMIT $2",
    )
    .bind(id)
    .bind(MAX_FAILURES_LISTED)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(remote_id, error)| LibraryFailure { remote_id, error })
            .collect(),
    ))
}

/// Runs for the life of the server. Imports left running by a restart go on.
pub async fn run_library_imports(state: AppState) {
    if state.fetcher.is_none() {
        return;
    }
    if let Err(e) =
        sqlx::query("UPDATE library_imports SET state = 'queued' WHERE state = 'running'")
            .execute(&state.db)
            .await
    {
        warn!("cannot requeue library imports: {e}");
    }
    loop {
        let woken = state.library_imports.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();
        match run_next(&state).await {
            Ok(true) => {}
            Ok(false) => {
                let _ = tokio::time::timeout(IDLE, woken).await;
            }
            Err(e) => {
                warn!("library import worker: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// An import being worked on.
#[derive(FromRow)]
struct Job {
    id: i64,
    user_id: i64,
    source: String,
    base: String,
    login: Option<String>,
    secret: Option<String>,
    unrated: i16,
    phase: String,
    position: String,
    total: Option<i64>,
}

/// How a walk came out.
enum End {
    Done,
    /// Someone stopped or removed it: already written down.
    Stopped,
    Failed(String),
}

async fn run_next(state: &AppState) -> AppResult<bool> {
    let job: Option<Job> = sqlx::query_as(
        "UPDATE library_imports SET state = 'running' WHERE id = ( \
             SELECT id FROM library_imports WHERE state = 'queued' ORDER BY id LIMIT 1 \
             FOR UPDATE SKIP LOCKED \
         ) RETURNING id, user_id, source, base, login, secret, unrated, phase, position, total",
    )
    .fetch_optional(&state.db)
    .await?;
    let Some(job) = job else { return Ok(false) };
    let id = job.id;
    info!("library import {id} starting");
    let end = walk(state, job).await.unwrap_or_else(|e| {
        // Details may name a file on disk, and never leave the log.
        error!("library import {id}: {e}");
        End::Failed("the server failed while storing posts; see its log".to_owned())
    });
    let (new_state, message) = match end {
        End::Stopped => return Ok(true),
        End::Done => ("done", None),
        End::Failed(message) => ("failed", Some(message)),
    };
    sqlx::query(
        "UPDATE library_imports SET state = $2, error = $3, login = NULL, secret = NULL, \
         finished_at = now() WHERE id = $1 AND state = 'running'",
    )
    .bind(id)
    .bind(new_state)
    .bind(message)
    .execute(&state.db)
    .await?;
    info!("library import {id}: {new_state}");
    Ok(true)
}

/// Whether the import may go on, and with what reach.
enum Standing {
    Going { admin: bool },
    Stopped,
    Disabled,
}

async fn standing(state: &AppState, job: &Job) -> AppResult<Standing> {
    let row: Option<(String, String, bool)> = sqlx::query_as(
        "SELECT i.state, u.role, u.disabled_at IS NOT NULL FROM library_imports i \
         JOIN users u ON u.id = i.user_id WHERE i.id = $1",
    )
    .bind(job.id)
    .fetch_optional(&state.db)
    .await?;
    Ok(match row {
        Some((import, _, _)) if import != "running" => Standing::Stopped,
        None => Standing::Stopped,
        Some((_, _, true)) => Standing::Disabled,
        Some((_, role, false)) => Standing::Going {
            admin: Role::from_db(&role) == Some(Role::Admin),
        },
    })
}

async fn add_note(state: &AppState, id: i64, note: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE library_imports SET note = CASE WHEN note IS NULL THEN $2 \
         WHEN position($2 in note) > 0 THEN note \
         ELSE note || ' ' || $2 END WHERE id = $1",
    )
    .bind(id)
    .bind(note)
    .execute(&state.db)
    .await?;
    Ok(())
}

async fn save_place(
    state: &AppState,
    id: i64,
    phase: LibraryPhase,
    position: &str,
    total: Option<u64>,
) -> AppResult<()> {
    // Some systems count only what's on the page, so the empty page that ends a
    // walk says 0: that doesn't undo a count already known.
    sqlx::query(
        "UPDATE library_imports SET phase = $2, position = $3, \
         total = COALESCE(NULLIF($4, 0), total, $4) WHERE id = $1",
    )
    .bind(id)
    .bind(phase.as_str())
    .bind(position)
    .bind(total.and_then(|n| i64::try_from(n).ok()))
    .execute(&state.db)
    .await?;
    Ok(())
}

fn page_print(scope: &str, body: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    scope.hash(&mut hasher);
    body.hash(&mut hasher);
    hasher.finish()
}

async fn walk(state: &AppState, job: Job) -> AppResult<End> {
    let source = LibrarySource::parse(&job.source)
        .ok_or_else(|| AppError::internal("unknown library source in the database"))?;
    let Some(site) = Site::new(source, &job.base, job.login.clone(), job.secret.clone()) else {
        return Ok(End::Failed(
            "that address or key has characters that can't be sent".to_owned(),
        ));
    };
    let site = Arc::new(site);
    let unrated = Rating::from_db(job.unrated).unwrap_or(Rating::Questionable);
    let mut phase = LibraryPhase::parse(&job.phase).unwrap_or(LibraryPhase::Posts);
    let mut position = job.position.clone();
    let mut counted = job.total.is_some();
    let mut last_page = None;
    let mut lost = 0;
    loop {
        let admin = match standing(state, &job).await? {
            Standing::Stopped => return Ok(End::Stopped),
            Standing::Disabled => return Ok(End::Failed("this account was disabled".to_owned())),
            Standing::Going { admin } => admin,
        };
        // Aliases and implications are shared by the whole server.
        let call = if phase == LibraryPhase::Rules && !admin {
            None
        } else {
            site.call(phase, &position)
        };
        let Some(call) = call else {
            match phase.next() {
                Some(next) => {
                    phase = next;
                    position.clear();
                    save_place(state, job.id, phase, &position, None).await?;
                    continue;
                }
                None => return Ok(End::Done),
            }
        };
        let fetcher = fetcher_for(state, admin)?;
        if phase == LibraryPhase::Posts && !counted {
            counted = true;
            if let Some(call) = site.count_call()
                && let Ok(body) = send(fetcher, &site, &call).await
            {
                save_place(state, job.id, phase, &position, site.parse_count(&body)).await?;
            }
        }
        // Rules and groups are extras: losing one is a note. Posts are the point.
        let required = phase == LibraryPhase::Posts;
        let body = match send(fetcher, &site, &call).await {
            Ok(body) => body,
            Err(e @ FetchError::Status(401 | 403 | 404)) if !required => {
                let note = format!("{} left out: {}.", what_is_read(phase), explain(source, &e));
                add_note(state, job.id, &note).await?;
                let next = site.skip_step(phase, &position);
                (phase, position) = moved_on(phase, next);
                save_place(state, job.id, phase, &position, None).await?;
                continue;
            }
            Err(e) => return Ok(End::Failed(explain(source, &e))),
        };
        let Some(answer) = site.parse(phase, &position, &body) else {
            if required {
                return Ok(End::Failed(not_that_system(source)));
            }
            let note = format!(
                "{} left out: {}.",
                what_is_read(phase),
                not_that_system(source)
            );
            add_note(state, job.id, &note).await?;
            let next = site.skip_step(phase, &position);
            (phase, position) = moved_on(phase, next);
            save_place(state, job.id, phase, &position, None).await?;
            continue;
        };
        let print = page_print(&answer.scope, &body);
        if !answer.is_empty() && last_page == Some(print) {
            let stuck = "the site keeps sending the same page".to_owned();
            if required {
                return Ok(End::Failed(stuck));
            }
            add_note(
                state,
                job.id,
                &format!("{} left out: {stuck}.", what_is_read(phase)),
            )
            .await?;
            let next = site.skip_step(phase, &position);
            (phase, position) = moved_on(phase, next);
            save_place(state, job.id, phase, &position, None).await?;
            continue;
        }
        last_page = Some(print);
        if answer.total.is_some() {
            counted = true;
        }
        let Answer {
            items,
            skipped,
            rules,
            groups,
            likes,
            next,
            total,
            ..
        } = answer;
        // Said before the page's files come down, which can take a good while.
        if total.is_some() {
            save_place(state, job.id, phase, &position, total).await?;
        }
        if !rules.is_empty() {
            let added = import_rules(state, &rules).await?;
            sqlx::query("UPDATE library_imports SET rules = rules + $2 WHERE id = $1")
                .bind(job.id)
                .bind(i64::try_from(added).unwrap_or(0))
                .execute(&state.db)
                .await?;
        }
        for (remote_id, reason) in skipped {
            record_failure(state, job.id, &remote_id, &reason).await?;
        }
        if let Some(end) = store_items(state, &job, &site, admin, unrated, items, &mut lost).await?
        {
            return Ok(end);
        }
        apply_groups(state, &job, &groups, &likes).await?;
        (phase, position) = moved_on(phase, next);
        save_place(state, job.id, phase, &position, total).await?;
    }
}

/// The next place from what an answer said: further along, or the next phase.
/// Past the last phase, the position stays empty and `call` finds nothing left.
fn moved_on(phase: LibraryPhase, next: Option<String>) -> (LibraryPhase, String) {
    match (next, phase.next()) {
        (Some(position), _) => (phase, position),
        (None, Some(after)) => (after, String::new()),
        (None, None) => (phase, FINISHED.to_owned()),
    }
}

/// A position no source knows, so nothing is left to ask for.
const FINISHED: &str = "finished:";

fn what_is_read(phase: LibraryPhase) -> &'static str {
    match phase {
        LibraryPhase::Rules => "Aliases and implications",
        LibraryPhase::Posts => "Posts",
        LibraryPhase::Groups => "Pools and favourites",
    }
}

async fn record_failure(state: &AppState, id: i64, remote_id: &str, reason: &str) -> AppResult<()> {
    record(state, id, remote_id, None, Some(reason), "failed").await
}

/// Writes down what became of a post, and adds it to its counter.
async fn record(
    state: &AppState,
    id: i64,
    remote_id: &str,
    post_id: Option<i64>,
    failure: Option<&str>,
    counter: &str,
) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO library_import_items (import_id, remote_id, post_id, error) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (import_id, remote_id) \
         DO UPDATE SET post_id = EXCLUDED.post_id, error = EXCLUDED.error",
    )
    .bind(id)
    .bind(remote_id)
    .bind(post_id)
    .bind(failure)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE library_imports SET imported = imported + ($2 = 'imported')::int, \
         merged = merged + ($2 = 'merged')::int, failed = failed + ($2 = 'failed')::int \
         WHERE id = $1",
    )
    .bind(id)
    .bind(counter)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

type Download = (Item, JoinHandle<Result<Vec<u8>, FetchError>>);

/// Downloads under way; those left when the import ends are called off.
struct Downloads(VecDeque<Download>);

impl Drop for Downloads {
    fn drop(&mut self) {
        for (_, handle) in &self.0 {
            handle.abort();
        }
    }
}

/// Starts fetching a post's file on its own task.
fn start_download(state: &AppState, site: &Arc<Site>, admin: bool, item: Item) -> Download {
    let state = state.clone();
    let site = site.clone();
    let url = site.file_address(&item.file_url);
    let handle = tokio::spawn(async move {
        let fetcher = fetcher_for(&state, admin).map_err(|_| FetchError::Blocked)?;
        let max = state.config.max_upload_bytes;
        retrying(|| {
            fetcher.call(
                SiteCall {
                    url: url.clone(),
                    body: None,
                    site: &site.base,
                    headers: site.headers(),
                    json: false,
                },
                max,
            )
        })
        .await
    });
    (item, handle)
}

/// Stores a page of posts, in the order the site lists them, so ids here keep
/// their order. `Some` ends the import.
async fn store_items(
    state: &AppState,
    job: &Job,
    site: &Arc<Site>,
    admin: bool,
    unrated: Rating,
    items: Vec<Item>,
    lost: &mut u32,
) -> AppResult<Option<End>> {
    if items.is_empty() {
        return Ok(None);
    }
    let ids: Vec<&str> = items.iter().map(|item| item.remote_id.as_str()).collect();
    let done: Vec<String> = sqlx::query_scalar(
        "SELECT remote_id FROM library_import_items \
         WHERE import_id = $1 AND remote_id = ANY($2) AND error IS NULL",
    )
    .bind(job.id)
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    let mut waiting = items
        .into_iter()
        .filter(|item| !done.contains(&item.remote_id));
    let mut downloads = Downloads(VecDeque::new());
    loop {
        while downloads.0.len() < DOWNLOADS {
            let Some(item) = waiting.next() else { break };
            downloads
                .0
                .push_back(start_download(state, site, admin, item));
        }
        let Some((item, handle)) = downloads.0.pop_front() else {
            return Ok(None);
        };
        match standing(state, job).await? {
            Standing::Going { .. } => {}
            Standing::Stopped => {
                handle.abort();
                return Ok(Some(End::Stopped));
            }
            Standing::Disabled => {
                handle.abort();
                return Ok(Some(End::Failed("this account was disabled".to_owned())));
            }
        }
        let fetched = handle.await.map_err(AppError::internal)?;
        let bytes = match fetched {
            Ok(bytes) => {
                *lost = 0;
                bytes
            }
            Err(e) => {
                if passing(&e) {
                    *lost = lost.saturating_add(1);
                    if *lost >= MAX_LOST {
                        return Ok(Some(End::Failed(format!(
                            "that site stopped answering ({e})"
                        ))));
                    }
                }
                record_failure(state, job.id, &item.remote_id, &e.to_string()).await?;
                continue;
            }
        };
        if let Some(end) = store_item(state, job, unrated, &item, bytes).await? {
            return Ok(Some(end));
        }
    }
}

/// Stores one post. A file refused for being what it is counts as that post's
/// failure; running out of room ends the import.
async fn store_item(
    state: &AppState,
    job: &Job,
    unrated: Rating,
    item: &Item,
    bytes: Vec<u8>,
) -> AppResult<Option<End>> {
    let stored = store_post(
        state,
        job.user_id,
        Bytes::from(bytes),
        &PostFields {
            tags: item.tags.clone(),
            rating: item.rating.unwrap_or(unrated),
            source: item.source.clone(),
            autotag: None,
        },
    )
    .await;
    let (post_id, counter) = match stored {
        Ok(Stored::Added(id)) => {
            if let Some(date) = item.created_at {
                sqlx::query("UPDATE posts SET created_at = $2 WHERE id = $1")
                    .bind(id)
                    .bind(date)
                    .execute(&state.db)
                    .await?;
            }
            (id, "imported")
        }
        Ok(Stored::Exists(id)) => {
            merge_into(state, id, item.source.as_deref(), &item.tags).await?;
            (id, "merged")
        }
        Err(AppError::QuotaExceeded) => {
            return Ok(Some(End::Failed(AppError::QuotaExceeded.to_string())));
        }
        Err(e @ AppError::Internal(_)) => return Err(e),
        Err(e) => {
            record_failure(state, job.id, &item.remote_id, &e.to_string()).await?;
            return Ok(None);
        }
    };
    if item.vote != 0 {
        import_vote(state, post_id, job.user_id, item.vote).await?;
    }
    record(state, job.id, &item.remote_id, Some(post_id), None, counter).await?;
    Ok(None)
}

/// The posts this import brought over from `remote_ids`.
async fn posts_of(state: &AppState, id: i64, remote_ids: &[String]) -> AppResult<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT post_id FROM library_import_items \
         WHERE import_id = $1 AND remote_id = ANY($2) AND post_id IS NOT NULL",
    )
    .bind(id)
    .bind(remote_ids)
    .fetch_all(&state.db)
    .await?)
}

async fn apply_groups(
    state: &AppState,
    job: &Job,
    groups: &[Group],
    likes: &[String],
) -> AppResult<()> {
    for group in groups {
        let tag = std::slice::from_ref(&group.tag);
        for post in posts_of(state, job.id, &group.remote_ids).await? {
            merge_into(state, post, None, tag).await?;
        }
    }
    if !likes.is_empty() {
        for post in posts_of(state, job.id, likes).await? {
            import_vote(state, post, job.user_id, 1).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use axum::Router;
    use axum::extract::Query;
    use axum::http::{HeaderMap, Method};
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;
    use serde_json::{Value, json};
    use starbooru_core::Post;

    use super::*;
    use crate::testing;

    async fn app() -> Option<testing::TestApp> {
        testing::app_with(|c| {
            c.url_import = Some(crate::fetch::Reach::Any);
            c.max_upload_bytes = 1024 * 1024;
        })
        .await
    }

    /// A LANBooru: two posts a page, with files that can change between runs.
    #[derive(Clone, Default)]
    struct Fake {
        images: Arc<Mutex<Vec<Value>>>,
        files: Arc<Mutex<HashMap<i64, Vec<u8>>>>,
    }

    impl Fake {
        fn add(&self, id: i64, file: Vec<u8>, extra: Value) {
            let mut image = json!({"id": id, "rating": 1, "tags": []});
            if let (Some(image), Some(extra)) = (image.as_object_mut(), extra.as_object()) {
                image.extend(extra.clone());
            }
            self.images.lock().unwrap().push(image);
            self.files.lock().unwrap().insert(id, file);
        }

        async fn serve(self) -> String {
            let search = {
                let fake = self.clone();
                move |Query(q): Query<HashMap<String, String>>| {
                    let fake = fake.clone();
                    async move {
                        let page: usize = q.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
                        let images = fake.images.lock().unwrap().clone();
                        let shown: Vec<Value> = images
                            .iter()
                            .skip((page - 1) * 2)
                            .take(2)
                            .cloned()
                            .collect();
                        Json(json!({"total": images.len(), "images": shown}))
                    }
                }
            };
            let file = {
                let fake = self.clone();
                move |Path(id): Path<i64>| {
                    let fake = fake.clone();
                    async move {
                        match fake.files.lock().unwrap().get(&id) {
                            Some(bytes) => bytes.clone().into_response(),
                            None => StatusCode::NOT_FOUND.into_response(),
                        }
                    }
                }
            };
            let router = Router::new()
                .route("/api/search", get(search))
                .route("/api/image/{id}/file", get(file));
            serve(router).await
        }
    }

    async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await });
        format!("http://{addr}")
    }

    fn lan(base: &str) -> Value {
        json!({"source": "lanbooru", "base": base, "unrated": "explicit"})
    }

    async fn start(app: &testing::TestApp, token: &str, site: Value) -> i64 {
        let r = app
            .call(
                Method::POST,
                "/api/v1/library-imports",
                Some(token),
                Some(site),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED);
        let import: LibraryImport = serde_json::from_value(r.json()).unwrap();
        import.id
    }

    /// Runs what's queued right here, instead of in the background worker.
    async fn drain(app: &testing::TestApp) {
        while run_next(&app.state).await.unwrap() {}
    }

    async fn import_of(app: &testing::TestApp, token: &str, id: i64) -> LibraryImport {
        let all: Vec<LibraryImport> =
            serde_json::from_value(app.get("/api/v1/library-imports", token).await.json()).unwrap();
        all.into_iter().find(|i| i.id == id).unwrap()
    }

    async fn posts(app: &testing::TestApp, token: &str) -> Vec<Post> {
        let page = app.get("/api/v1/posts?limit=50", token).await.json();
        serde_json::from_value(page.get("posts").unwrap().clone()).unwrap()
    }

    #[tokio::test]
    async fn imports_everything_keeping_order_rating_tags_and_date() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let fake = Fake::default();
        fake.add(
            1,
            testing::png(4, 4, 1),
            json!({
                "rating": 3, "tags": ["blue_sky"], "artist": "Some One",
                "source": "https://a.b/1", "created_at": "2001-02-03T04:05:06Z"
            }),
        );
        fake.add(2, testing::png(4, 4, 2), json!({"rating": 0}));
        fake.add(3, testing::png(4, 4, 3), json!({"rating": 1}));
        fake.add(4, b"not a picture".to_vec(), json!({}));
        let base = fake.serve().await;

        let check = app
            .call(
                Method::POST,
                "/api/v1/library-imports/check",
                Some(&token),
                Some(lan(&base)),
            )
            .await;
        assert_eq!(check.status, StatusCode::OK);
        let check: LibraryCheck = serde_json::from_value(check.json()).unwrap();
        assert_eq!(
            check,
            LibraryCheck {
                total: Some(4),
                rules: true
            }
        );

        let id = start(&app, &token, lan(&base)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Done, "{:?}", import.error);
        assert_eq!((import.imported, import.merged, import.failed), (3, 0, 1));
        assert_eq!(import.total, Some(4));

        let mut found = posts(&app, &token).await;
        found.sort_by_key(|p| p.id);
        let ratings: Vec<Rating> = found.iter().map(|p| p.rating).collect();
        assert_eq!(ratings, [Rating::Explicit, Rating::Explicit, Rating::Safe]);
        let first = found.first().unwrap();
        assert_eq!(first.source.as_deref(), Some("https://a.b/1"));
        assert_eq!(first.created_at.to_rfc3339(), "2001-02-03T04:05:06+00:00");
        let tags = &first.tags;
        assert!(
            tags.iter().any(|t| t == "blue_sky") && tags.iter().any(|t| t == "some_one"),
            "{tags:?}"
        );

        let failures = app
            .get(&format!("/api/v1/library-imports/{id}/failures"), &token)
            .await;
        let failures: Vec<LibraryFailure> = serde_json::from_value(failures.json()).unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures.first().unwrap().remote_id, "4");
        app.finish().await;
    }

    #[tokio::test]
    async fn a_second_import_merges_tags_and_leaves_the_bin_alone() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let kept = app.upload_png(&token, 11, "mine").await;
        let binned = app.upload_png(&token, 12, "old").await;
        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/posts/{binned}"),
                Some(&token),
                None,
            )
            .await;
        assert!(r.status.is_success());

        let fake = Fake::default();
        fake.add(
            1,
            testing::png(8 + 11, 8, 11),
            json!({"tags": ["theirs"], "source": "https://a.b/1"}),
        );
        fake.add(2, testing::png(8 + 12, 8, 12), json!({"tags": ["theirs"]}));
        let id = start(&app, &token, lan(&fake.serve().await)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(
            (import.imported, import.merged, import.failed),
            (0, 2, 0),
            "{:?}",
            import.error
        );

        let post = app
            .get(&format!("/api/v1/posts/{kept}"), &token)
            .await
            .json();
        let post: Post = serde_json::from_value(post).unwrap();
        let tags = &post.tags;
        assert!(
            tags.iter().any(|t| t == "mine") && tags.iter().any(|t| t == "theirs"),
            "{tags:?}"
        );
        assert_eq!(post.source.as_deref(), Some("https://a.b/1"));
        let binned_tags: i32 =
            sqlx::query_scalar("SELECT cardinality(tag_ids) FROM posts WHERE id = $1")
                .bind(binned)
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(binned_tags, 1, "a binned post stays as it was");
        app.finish().await;
    }

    #[tokio::test]
    async fn stop_clears_the_account_and_retry_goes_on_from_what_failed() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let fake = Fake::default();
        fake.add(1, testing::png(4, 4, 21), json!({}));
        fake.add(2, b"broken".to_vec(), json!({}));
        let base = fake.clone().serve().await;

        let mut site = lan(&base);
        site["source"] = json!("stash");
        site["secret"] = json!("hunter2");
        let stopped = start(&app, &token, site).await;
        let secret: Option<String> =
            sqlx::query_scalar("SELECT secret FROM library_imports WHERE id = $1")
                .bind(stopped)
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(
            secret.as_deref(),
            Some("hunter2"),
            "kept to survive a restart"
        );
        let listed = app.get("/api/v1/library-imports", &token).await;
        assert!(
            !listed.json().to_string().contains("hunter2"),
            "never sent back"
        );
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/library-imports/{stopped}/stop"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let row: (String, Option<String>) =
            sqlx::query_as("SELECT state, secret FROM library_imports WHERE id = $1")
                .bind(stopped)
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(row, ("stopped".to_owned(), None));

        let id = start(&app, &token, lan(&base)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!((import.imported, import.failed), (1, 1));

        // The broken file gets fixed and a new one appears: only those download.
        let fixed = testing::png(4, 4, 22);
        fake.files.lock().unwrap().insert(2, fixed);
        fake.add(3, testing::png(4, 4, 23), json!({}));
        fake.files.lock().unwrap().remove(&1);
        let r = app
            .call(
                Method::POST,
                &format!("/api/v1/library-imports/{id}/retry"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Done, "{:?}", import.error);
        assert_eq!((import.imported, import.failed), (3, 0));

        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/library-imports/{id}"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        assert_eq!(posts(&app, &token).await.len(), 3, "its posts stay");
        app.finish().await;
    }

    #[tokio::test]
    async fn each_account_sees_only_its_own_and_rules_are_for_administrators() {
        let Some(app) = app().await else { return };
        let admin = app.register("admin").await;
        let other = app.register("other").await;

        let router = Router::new().route(
            "/api/search",
            get(|| async { Json(json!({"total": 0, "images": []})) }),
        );
        let base = serve(router).await;
        let check = app
            .call(
                Method::POST,
                "/api/v1/library-imports/check",
                Some(&other),
                Some(lan(&base)),
            )
            .await;
        let check: LibraryCheck = serde_json::from_value(check.json()).unwrap();
        assert!(!check.rules);

        let id = start(&app, &admin, lan(&base)).await;
        let path = format!("/api/v1/library-imports/{id}/stop");
        let r = app.call(Method::POST, &path, Some(&other), None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app
            .get(&format!("/api/v1/library-imports/{id}/failures"), &other)
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app
            .call(
                Method::DELETE,
                &format!("/api/v1/library-imports/{id}"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT, "still going");
        app.finish().await;
    }

    #[tokio::test]
    async fn ordinary_accounts_cannot_reach_the_servers_network() {
        let Some(app) = testing::app_with(|c| {
            c.url_import = Some(crate::fetch::Reach::Public);
        })
        .await
        else {
            return;
        };
        let admin = app.register("admin").await;
        let other = app.register("other").await;
        let fake = Fake::default();
        let base = fake.serve().await;

        let r = app
            .call(
                Method::POST,
                "/api/v1/library-imports/check",
                Some(&other),
                Some(lan(&base)),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "private address refused");

        let r = app
            .call(
                Method::POST,
                "/api/v1/library-imports/check",
                Some(&admin),
                Some(lan(&base)),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "the administrator set this up");

        let id = start(&app, &other, lan(&base)).await;
        drain(&app).await;
        let import = import_of(&app, &other, id).await;
        assert_eq!(import.state, LibraryState::Failed);
        assert!(import.error.is_some());
        app.finish().await;
    }

    /// A Danbooru that lists rules, one post on another host, pools and favourites.
    async fn fake_danbooru(cdn: String, seen: Arc<Mutex<Vec<String>>>) -> String {
        let posts = move |Query(q): Query<HashMap<String, String>>| {
            let cdn = cdn.clone();
            async move {
                let page = q.get("page").map(String::as_str);
                if page != Some("a0") {
                    return Json(json!([]));
                }
                if q.get("tags").is_some_and(|t| t.starts_with("fav:")) {
                    return Json(json!([{"id": 1}]));
                }
                Json(json!([{
                    "id": 1, "rating": "s", "file_url": format!("{cdn}/a.png"),
                    "tag_string_general": "old_name", "tag_string_artist": "someone",
                    "created_at": "2010-01-01T00:00:00Z"
                }]))
            }
        };
        let saw = seen.clone();
        let aliases = move |headers: HeaderMap, Query(q): Query<HashMap<String, String>>| {
            let saw = saw.clone();
            async move {
                if let Some(auth) = headers.get("authorization").and_then(|a| a.to_str().ok()) {
                    saw.lock().unwrap().push(auth.to_owned());
                }
                if q.get("page").map(String::as_str) != Some("a0") {
                    return Json(json!([]));
                }
                Json(json!([
                    {"id": 1, "antecedent_name": "old_name", "consequent_name": "new_name"},
                    {"id": 2, "antecedent_name": "taken", "consequent_name": "new_name"}
                ]))
            }
        };
        let implications = |Query(q): Query<HashMap<String, String>>| async move {
            if q.get("page").map(String::as_str) != Some("a0") {
                return Json(json!([]));
            }
            Json(json!([
                {"id": 1, "antecedent_name": "new_name", "consequent_name": "implied"},
                {"id": 2, "antecedent_name": "implied", "consequent_name": "new_name"}
            ]))
        };
        let pools = |Query(q): Query<HashMap<String, String>>| async move {
            if q.get("page").map(String::as_str) != Some("a0") {
                return Json(json!([]));
            }
            Json(json!([{"id": 1, "name": "A_Trip", "post_ids": [1, 99]}]))
        };
        let router = Router::new()
            .route("/posts.json", get(posts))
            .route("/tag_aliases.json", get(aliases))
            .route("/tag_implications.json", get(implications))
            .route("/pools.json", get(pools))
            .route(
                "/counts/posts.json",
                get(|| async { Json(json!({"counts": {"posts": 1}})) }),
            );
        serve(router).await
    }

    #[tokio::test]
    async fn rules_pools_likes_and_the_key_stays_off_the_cdn() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        app.upload_png(&token, 31, "taken").await;
        let cdn_saw: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = cdn_saw.clone();
        let png = testing::png(4, 4, 32);
        let cdn = serve(Router::new().route(
            "/a.png",
            get(move |headers: HeaderMap| {
                let log = log.clone();
                let png = png.clone();
                async move {
                    let auth = headers.get("authorization").is_some();
                    log.lock().unwrap().push(auth.to_string());
                    png.into_response() as Response
                }
            }),
        ))
        .await;
        let site_saw: Arc<Mutex<Vec<String>>> = Arc::default();
        let base = fake_danbooru(cdn, site_saw.clone()).await;
        let id = start(
            &app,
            &token,
            json!({"source": "danbooru", "base": base, "login": "me", "secret": "key"}),
        )
        .await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Done, "{:?}", import.error);
        assert_eq!(import.imported, 1);
        assert_eq!(import.total, Some(1));
        assert_eq!(
            import.rules, 2,
            "one alias and one implication; the rest would overrule or loop"
        );

        assert!(
            site_saw
                .lock()
                .unwrap()
                .iter()
                .all(|a| a == "Basic bWU6a2V5")
        );
        assert!(!site_saw.lock().unwrap().is_empty());
        assert_eq!(
            *cdn_saw.lock().unwrap(),
            ["false"],
            "no key on another host"
        );

        let found = posts(&app, &token).await;
        let post = found
            .iter()
            .find(|p| p.rating == Rating::Sensitive)
            .unwrap();
        let tags = &post.tags;
        for tag in ["new_name", "implied", "someone", "pool_a_trip"] {
            assert!(tags.iter().any(|t| t == tag), "{tag} in {tags:?}");
        }
        assert!(!tags.iter().any(|t| t == "old_name"), "the alias resolved");
        assert_eq!(post.score, 1, "favourite comes across as a vote");

        let aliased: i64 =
            sqlx::query_scalar("SELECT count(*) FROM tag_aliases WHERE alias = 'taken'")
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(aliased, 0, "an existing tag isn't merged away");
        let creds: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT login, secret FROM library_imports WHERE id = $1")
                .bind(id)
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(creds, (None, None), "gone once it ends");
        app.finish().await;
    }

    #[tokio::test]
    async fn pages_of_rules_it_cannot_read_are_a_note_not_a_failure() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let posts = get(|Query(q): Query<HashMap<String, String>>| async move {
            if q.get("page").map(String::as_str) != Some("a0") {
                return Json(json!([]));
            }
            Json(json!([{"id": 1, "rating": "g", "tag_string_general": "x"}]))
        });
        let router = Router::new()
            .route("/posts.json", posts)
            .route("/tag_aliases.json", get(|| async { StatusCode::FORBIDDEN }))
            .route("/tag_implications.json", get(|| async { "<html>" }))
            .route("/pools.json", get(|| async { StatusCode::NOT_FOUND }));
        let base = serve(router).await;
        let id = start(&app, &token, json!({"source": "danbooru", "base": base})).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Done, "{:?}", import.error);
        let note = import.note.unwrap();
        assert!(note.contains("Aliases and implications left out"), "{note}");
        assert!(note.contains("Pools and favourites left out"), "{note}");
        assert_eq!(import.failed, 1, "a post hidden from this account");
        app.finish().await;
    }

    #[tokio::test]
    async fn failing_posts_fail_clearly_and_a_stuck_site_is_noticed() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;

        let refused =
            serve(Router::new().route("/api/search", get(|| async { StatusCode::UNAUTHORIZED })))
                .await;
        let id = start(&app, &token, lan(&refused)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Failed);
        assert!(import.error.unwrap().contains("refused this account"));

        let html = serve(Router::new().route("/api/search", get(|| async { "<html>" }))).await;
        let id = start(&app, &token, lan(&html)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert!(
            import
                .error
                .unwrap()
                .contains("doesn't answer like LANBooru")
        );

        // Ignores `page`: without noticing, this would go on forever.
        let stuck = serve(Router::new().route(
            "/api/search",
            get(|| async { Json(json!({"total": 1, "images": [{"id": 1}]})) }),
        ))
        .await;
        let id = start(&app, &token, lan(&stuck)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert!(import.error.unwrap().contains("same page"));
        app.finish().await;
    }

    #[tokio::test]
    async fn it_goes_on_after_a_restart_and_stops_for_a_disabled_account() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let other = app.register("other").await;
        let fake = Fake::default();
        fake.add(1, testing::png(4, 4, 41), json!({}));
        fake.add(2, testing::png(4, 4, 42), json!({}));
        fake.add(3, testing::png(4, 4, 43), json!({}));
        let base = fake.serve().await;

        // Stopped mid-walk by a restart: the first page is done, the place saved.
        let id = start(&app, &token, lan(&base)).await;
        sqlx::query(
            "UPDATE library_imports SET state = 'running', phase = 'posts', position = 'posts:2'              WHERE id = $1",
        )
        .bind(id)
        .execute(app.db())
        .await
        .unwrap();
        tokio::spawn(run_library_imports(app.state.clone()));
        let mut waited = 0;
        while import_of(&app, &token, id).await.state.is_active() && waited < 200 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            waited += 1;
        }
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Done, "{:?}", import.error);
        assert_eq!(import.imported, 1, "only the page it hadn't reached");

        let id = start(&app, &other, lan(&base)).await;
        sqlx::query("UPDATE users SET disabled_at = now() WHERE username = 'other'")
            .execute(app.db())
            .await
            .unwrap();
        // The worker from above picks this one up.
        let mut waited = 0;
        loop {
            let state: String =
                sqlx::query_scalar("SELECT state FROM library_imports WHERE id = $1")
                    .bind(id)
                    .fetch_one(app.db())
                    .await
                    .unwrap();
            if state == "failed" || state == "done" || waited >= 200 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            waited += 1;
        }
        let state: (String, Option<String>) =
            sqlx::query_as("SELECT state, error FROM library_imports WHERE id = $1")
                .bind(id)
                .fetch_one(app.db())
                .await
                .unwrap();
        assert_eq!(state.0, "failed");
        assert!(state.1.unwrap().contains("disabled"));
        app.finish().await;
    }

    #[tokio::test]
    async fn the_storage_limit_ends_the_import_with_its_reason() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let fake = Fake::default();
        fake.add(1, testing::png(4, 4, 51), json!({}));
        let base = fake.serve().await;
        sqlx::query("UPDATE users SET quota_bytes = 1")
            .execute(app.db())
            .await
            .unwrap();
        let id = start(&app, &token, lan(&base)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Failed);
        assert!(import.error.unwrap().contains("storage limit"));
        app.finish().await;
    }

    #[tokio::test]
    async fn the_empty_page_at_the_end_does_not_undo_the_count() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let search = |Query(q): Query<HashMap<String, String>>| async move {
            match q.get("page").map(String::as_str) {
                Some("1") => Json(json!({"total": 1, "images": [{"id": 1, "tags": []}]})),
                _ => Json(json!({"total": 0, "images": []})),
            }
        };
        let router = Router::new().route("/api/search", get(search)).route(
            "/api/image/1/file",
            get(|| async { testing::png(4, 4, 52).into_response() }),
        );
        let base = serve(router).await;
        let id = start(&app, &token, lan(&base)).await;
        drain(&app).await;
        let import = import_of(&app, &token, id).await;
        assert_eq!(import.state, LibraryState::Done);
        assert_eq!((import.total, import.imported), (Some(1), 1));
        app.finish().await;
    }

    #[tokio::test]
    async fn the_count_is_known_while_the_first_page_comes_down() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let seen = Arc::new(Mutex::new(None));
        let file = {
            let (db, seen) = (app.db().clone(), seen.clone());
            move || {
                let (db, seen) = (db.clone(), seen.clone());
                async move {
                    let total: Option<i64> =
                        sqlx::query_scalar("SELECT total FROM library_imports")
                            .fetch_one(&db)
                            .await
                            .unwrap();
                    *seen.lock().unwrap() = Some(total);
                    testing::png(4, 4, 53).into_response()
                }
            }
        };
        let search = |Query(q): Query<HashMap<String, String>>| async move {
            let images = match q.get("page").map(String::as_str) {
                Some("1") => json!([{"id": 1, "tags": []}]),
                _ => json!([]),
            };
            Json(json!({"total": 7, "images": images}))
        };
        let router = Router::new()
            .route("/api/search", get(search))
            .route("/api/image/1/file", get(file));
        let base = serve(router).await;
        start(&app, &token, lan(&base)).await;
        drain(&app).await;
        assert_eq!(*seen.lock().unwrap(), Some(Some(7)));
        app.finish().await;
    }

    #[tokio::test]
    async fn the_same_note_is_said_once() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let base = serve(Router::new()).await;
        let id = start(&app, &token, lan(&base)).await;
        for note in ["Aliases left out.", "Aliases left out.", "Pools left out."] {
            add_note(&app.state, id, note).await.unwrap();
        }
        let import = import_of(&app, &token, id).await;
        assert_eq!(
            import.note.as_deref(),
            Some("Aliases left out. Pools left out.")
        );
        app.finish().await;
    }

    #[tokio::test]
    async fn a_check_asks_once_and_says_what_went_wrong() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let search = {
            let calls = calls.clone();
            move || {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async { StatusCode::SERVICE_UNAVAILABLE }
            }
        };
        let base = serve(Router::new().route("/api/search", get(search))).await;
        let r = app
            .call(
                Method::POST,
                "/api/v1/library-imports/check",
                Some(&token),
                Some(lan(&base)),
            )
            .await;
        assert_eq!(r.status, StatusCode::FAILED_DEPENDENCY);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        app.finish().await;
    }

    #[tokio::test]
    async fn a_rate_limit_is_waited_out() {
        let calls = std::sync::atomic::AtomicU32::new(0);
        let answer = retrying(|| {
            let call = calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async move {
                if call < RATE_LIMIT_WAITS {
                    Err(FetchError::Status(429))
                } else {
                    Ok(vec![1])
                }
            }
        })
        .await;
        assert_eq!(
            answer.unwrap(),
            [1],
            "a site's limit isn't the post's fault"
        );

        let answer = retrying(|| async { Err::<Vec<u8>, _>(FetchError::Status(429)) }).await;
        assert!(
            matches!(answer, Err(FetchError::Status(429))),
            "but it does end"
        );
    }

    #[tokio::test]
    async fn refuses_bad_requests() {
        let Some(app) = app().await else { return };
        let token = app.register("admin").await;
        for site in [
            json!({"source": "stash", "base": "ftp://x"}),
            json!({"source": "danbooru", "base": "http://x", "secret": "only-a-key"}),
            json!({"source": "stash", "base": "http://x", "secret": "line\nbreak"}),
        ] {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/library-imports",
                    Some(&token),
                    Some(site),
                )
                .await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST);
        }
        let r = app
            .call(
                Method::POST,
                "/api/v1/library-imports",
                Some(&token),
                Some(json!({"source": "gelbooru", "base": "http://x"})),
            )
            .await;
        assert!(r.status.is_client_error());
        for _ in 0..MAX_ACTIVE_LIBRARY_IMPORTS {
            start(&app, &token, lan("http://127.0.0.1:9")).await;
        }
        let r = app
            .call(
                Method::POST,
                "/api/v1/library-imports",
                Some(&token),
                Some(lan("http://x")),
            )
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        app.finish().await;
    }
}
