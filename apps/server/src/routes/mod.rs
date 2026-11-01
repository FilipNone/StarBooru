//! HTTP routing. API reference: `docs/api.md`.

mod admin;
mod ai;
mod auth;
mod autotag;
mod backups;
mod duplicates;
mod edit;
mod explore;
mod figures;
mod imports;
mod keys;
mod library;
mod oidc;
mod posts;
mod remote;
mod searches;
mod shares;
mod tags;
mod trash;
mod uploads;

pub use autotag::backfill_meta_tags;
pub use imports::run_imports;
pub use library::run_library_imports;
pub use posts::{
    backfill_colors, backfill_file_dates, backfill_hashes, backfill_media, backfill_samples,
    backfill_thumbs,
};
pub use tags::seed_vocab;
pub use trash::auto_purge;

use std::any::Any;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use starbooru_core::{ApiError, ServerInfo};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any as AnyOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::{error, warn};

pub(crate) use figures::replace as replace_figures;
pub(crate) use posts::set_tags as set_post_tags;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let upload_limit = state.config.max_upload_bytes;
    let chunk_limit = usize::try_from(starbooru_core::UPLOAD_CHUNK_SIZE).unwrap_or(usize::MAX);

    let api = Router::new()
        .route("/info", get(info))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/usage", get(auth::usage))
        .route("/auth/password", put(auth::change_password))
        .route("/auth/oidc/begin", post(oidc::begin))
        .route("/auth/oidc/begin-link", post(oidc::begin_link))
        .route("/auth/oidc/callback", get(oidc::callback))
        .route("/auth/oidc/finish", post(oidc::finish))
        .route("/auth/oidc/link", post(oidc::link))
        .route("/auth/identities", get(oidc::identities))
        .route("/auth/identities/{id}", axum::routing::delete(oidc::unlink))
        .route(
            "/auth/sessions",
            get(auth::sessions).delete(auth::revoke_other_sessions),
        )
        .route(
            "/auth/sessions/{id}",
            axum::routing::delete(auth::revoke_session),
        )
        .route("/admin/stats", get(admin::stats))
        .route(
            "/admin/settings",
            get(admin::get_settings).put(admin::put_settings),
        )
        .route("/admin/ai", get(ai::admin_status).put(ai::put_general))
        .route("/admin/ai/tools/{tool}", axum::routing::put(ai::put_tool))
        .route("/admin/ai/tools/{tool}/test", post(ai::test_tool))
        .route("/admin/ai/models/{id}/install", post(ai::install_model))
        .route(
            "/admin/ai/models/{id}",
            axum::routing::delete(ai::uninstall_model),
        )
        .route("/ai", get(ai::status))
        .route("/posts/{id}/suggest-tags", post(ai::suggest_tags))
        .route("/posts/{id}/translate", post(ai::translate))
        .route(
            "/posts/{id}/translations",
            get(ai::panel_translations).post(ai::translate_panels),
        )
        .route("/admin/users", get(admin::users))
        .route(
            "/admin/users/{id}",
            axum::routing::patch(admin::update_user).delete(admin::delete_user),
        )
        .route("/admin/users/{id}/password", post(admin::reset_password))
        .route("/admin/backups", get(backups::list).post(backups::create))
        .route("/admin/backups/schedule", put(backups::set_schedule))
        .route(
            "/admin/backups/{name}",
            axum::routing::delete(backups::delete),
        )
        .route("/keys", get(keys::list).post(keys::create))
        .route("/keys/{id}", axum::routing::delete(keys::delete))
        .route(
            "/posts",
            get(posts::search)
                .post(posts::upload)
                .layer(DefaultBodyLimit::max(upload_limit)),
        )
        .route(
            "/posts/{id}",
            get(posts::get_one)
                .patch(posts::update)
                .delete(posts::delete),
        )
        .route("/posts/import", post(posts::import))
        .route("/uploads", post(uploads::start))
        .route("/uploads/{id}", axum::routing::delete(uploads::cancel))
        .route(
            "/uploads/{id}/chunks/{index}",
            put(uploads::put_chunk).layer(DefaultBodyLimit::max(chunk_limit)),
        )
        .route("/uploads/{id}/finish", post(uploads::finish))
        .route(
            "/imports",
            get(imports::list)
                .post(imports::create)
                .delete(imports::clear),
        )
        .route("/imports/{id}", axum::routing::delete(imports::delete))
        .route("/library-imports", get(library::list).post(library::create))
        .route("/library-imports/check", post(library::check))
        .route(
            "/library-imports/{id}",
            axum::routing::delete(library::delete),
        )
        .route("/library-imports/{id}/stop", post(library::stop))
        .route("/library-imports/{id}/retry", post(library::retry))
        .route("/library-imports/{id}/failures", get(library::failures))
        .route("/remote/search", post(remote::search))
        .route("/posts/bulk", post(posts::bulk))
        .route("/posts/{id}/neighbors", get(posts::neighbors))
        .route("/posts/{id}/related", get(posts::related))
        .route("/posts/{id}/restore", post(posts::restore))
        .route("/posts/{id}/ocr", post(posts::rescan_ocr))
        .route("/posts/{id}/purge", post(posts::purge))
        .route("/trash/empty", post(trash::empty))
        .route("/posts/{id}/vote", put(posts::vote))
        .route("/posts/{id}/figures", put(figures::put))
        .route("/posts/{id}/edit", post(edit::edit))
        .route("/shares", get(shares::list).post(shares::create))
        .route("/shares/{id}", axum::routing::delete(shares::delete))
        .route("/shared/{token}", get(shares::view))
        .route("/searches", get(searches::list).post(searches::create))
        .route("/searches/{id}", axum::routing::delete(searches::delete))
        .route("/explore", get(explore::explore))
        .route("/duplicates", get(duplicates::list))
        .route("/duplicates/resolve", post(duplicates::resolve))
        .route("/duplicates/dismiss", post(duplicates::dismiss))
        .route("/tags", get(tags::list))
        .route("/tag-corrections", get(tags::tag_corrections))
        .route("/tags/{name}", get(tags::detail).patch(tags::update))
        .route("/tags/{name}/aliases", post(tags::add_alias))
        .route(
            "/tags/{name}/aliases/{alias}",
            axum::routing::delete(tags::remove_alias),
        )
        .route("/tags/{name}/implications", post(tags::add_implication))
        .route(
            "/tags/{name}/implications/{implied}",
            axum::routing::delete(tags::remove_implication),
        )
        .fallback(api_not_found)
        .layer(CompressionLayer::new());

    // Native and Android clients run on other origins and authenticate with
    // bearer tokens (no cookies), so any origin may call the API.
    let cors = CorsLayer::new()
        .allow_origin(AnyOrigin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .expose_headers([header::RETRY_AFTER])
        .max_age(Duration::from_secs(3600));

    let mut app = Router::new()
        .nest(starbooru_core::API_PREFIX, api)
        .route("/media/{variant}/{file}", get(crate::media::serve))
        .route("/backups/{name}", get(backups::download))
        .route("/healthz", get(health));

    if let Some(dir) = &state.config.web_dir {
        if dir.join("index.html").is_file() {
            // Single-page app: unknown paths fall back to index.html.
            let spa = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
            app = app.fallback_service(
                tower::ServiceBuilder::new()
                    .layer(CompressionLayer::new())
                    .service(spa),
            );
        } else {
            warn!(
                "STARBOORU_WEB_DIR={} has no index.html; web app disabled",
                dir.display()
            );
        }
    }

    app.layer(axum::middleware::from_fn(detach))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::custom(panic_response))
        .with_state(state)
}

async fn info(State(state): State<AppState>) -> AppResult<Json<ServerInfo>> {
    let has_users: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&state.db)
        .await?;
    let settings = crate::settings::current(&state).await?;
    Ok(Json(ServerInfo {
        name: settings.name,
        version: starbooru_core::VERSION.to_owned(),
        has_users,
        registration_open: !has_users || settings.allow_registration,
        max_upload_bytes: state.config.max_upload_bytes as u64,
        url_import: state.fetcher.is_some(),
        oidc: state.oidc.as_ref().map(|o| o.name().to_owned()),
        video_editing: state.storage.has_tools(),
    }))
}

/// Liveness plus a database round trip, for Docker/Dokploy health checks.
async fn health(State(state): State<AppState>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (StatusCode::OK, "ok").into_response(),
        Err(e) => {
            warn!("health check: database unavailable: {e}");
            (StatusCode::SERVICE_UNAVAILABLE, "database unavailable").into_response()
        }
    }
}

async fn api_not_found() -> AppError {
    AppError::NotFound
}

/// Runs each request in its own task. Hyper drops a request's future when its
/// client goes away, which would stop a translation, an edit or an import
/// halfway; here only the wait is dropped, and the work finishes and is kept.
async fn detach(req: Request, next: Next) -> Response {
    match tokio::spawn(next.run(req)).await {
        Ok(response) => response,
        Err(e) if e.is_panic() => panic_response(e.into_panic()),
        Err(e) => {
            error!("request task stopped: {e}");
            AppError::Internal(e.to_string()).into_response()
        }
    }
}

/// Last-resort guard: a panic in a handler becomes a 500, never a dead server.
fn panic_response(err: Box<dyn Any + Send + 'static>) -> Response {
    let detail = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic");
    error!("handler panicked: {detail}");
    let mut resp = (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            code: "internal".into(),
            message: "internal server error".into(),
        }),
    )
        .into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    use crate::testing;

    /// The M1 API end to end: accounts, upload, search, edit, votes, bin.
    #[tokio::test]
    async fn core_api_flow() {
        let Some(app) = testing::app_with(|c| c.allow_registration = false).await else {
            return;
        };
        let info = app
            .call(Method::GET, "/api/v1/info", None, None)
            .await
            .json();
        assert_eq!(info["has_users"], false);
        let admin = app.register("admin").await;
        let me = app.get("/api/v1/auth/me", &admin).await.json();
        assert_eq!(me["role"], "admin");
        // Registration closes after the first account.
        let r = app
            .call(
                Method::POST,
                "/api/v1/auth/register",
                None,
                Some(json!({"username": "late", "password": "password123"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);

        let a = app.upload_png(&admin, 10, "blue_sky night").await;
        let b = app.upload_png(&admin, 11, "blue_sky cloud").await;
        let dup = app.upload(&admin, &testing::png(18, 8, 10), &[]).await;
        assert_eq!(dup.status, StatusCode::CONFLICT);
        let bad = app.upload(&admin, b"not an image", &[]).await;
        assert_eq!(bad.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let ids = |q: &str| {
            let q = q.to_owned();
            let app = &app;
            let admin = admin.clone();
            async move {
                let page = app
                    .get(&format!("/api/v1/posts?q={q}"), &admin)
                    .await
                    .json();
                page["posts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["id"].as_i64().unwrap())
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(ids("blue_sky").await, vec![b, a]);
        assert_eq!(ids("blue_sky+-cloud").await, vec![a]);
        assert_eq!(ids("missing_tag").await, Vec::<i64>::new());

        let r = app
            .call(
                Method::PATCH,
                &format!("/api/v1/posts/{a}"),
                Some(&admin),
                Some(json!({"tags": ["Night", "stars"], "rating": "questionable"})),
            )
            .await
            .json();
        assert_eq!(r["tags"], json!(["night", "stars"]));
        assert_eq!(r["rating"], "questionable");

        let r = app
            .call(
                Method::PUT,
                &format!("/api/v1/posts/{a}/vote"),
                Some(&admin),
                Some(json!({"value": 1})),
            )
            .await
            .json();
        assert_eq!(
            (r["score"].as_i64(), r["my_vote"].as_i64()),
            (Some(1), Some(1))
        );

        app.call(
            Method::DELETE,
            &format!("/api/v1/posts/{b}"),
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(ids("status:deleted").await, vec![b]);
        let tags = app.get("/api/v1/tags?q=blue", &admin).await.json();
        assert_eq!(tags[0]["post_count"], 0, "counts drop on delete");

        let post = app.get(&format!("/api/v1/posts/{a}"), &admin).await.json();
        let file = app
            .call(Method::GET, post["file_url"].as_str().unwrap(), None, None)
            .await;
        assert_eq!(file.status, StatusCode::OK);
        assert!(
            file.headers["cache-control"]
                .to_str()
                .unwrap()
                .contains("immutable")
        );

        let r = app
            .call(Method::POST, "/api/v1/auth/logout", Some(&admin), None)
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        assert_eq!(
            app.get("/api/v1/auth/me", &admin).await.status,
            StatusCode::UNAUTHORIZED
        );
        app.finish().await;
    }

    /// A client that hangs up mustn't stop the work it asked for.
    #[tokio::test]
    async fn work_finishes_after_the_client_leaves() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let make = |done: Arc<AtomicBool>, detached: bool| {
            let app = axum::Router::new().route(
                "/",
                axum::routing::post(move || async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    done.store(true, Ordering::SeqCst);
                }),
            );
            if detached {
                app.layer(axum::middleware::from_fn(super::detach))
            } else {
                app
            }
        };
        for detached in [false, true] {
            let done = Arc::new(AtomicBool::new(false));
            let request = Request::post("/").body(Body::empty()).unwrap();
            // the client gives up after 20 ms; dropping the future is what hyper does
            let answer = tokio::time::timeout(
                Duration::from_millis(20),
                make(done.clone(), detached).oneshot(request),
            )
            .await;
            assert!(answer.is_err());
            tokio::time::sleep(Duration::from_millis(250)).await;
            assert_eq!(done.load(Ordering::SeqCst), detached);
        }
    }

    #[tokio::test]
    async fn a_panic_in_a_detached_request_is_a_500() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        async fn boom() -> &'static str {
            panic!("boom")
        }
        let app = axum::Router::new()
            .route("/", axum::routing::get(boom))
            .layer(axum::middleware::from_fn(super::detach));
        let response = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
