//! HTTP routing. API reference: `docs/api.md`.

mod auth;
mod posts;
mod tags;

use std::any::Any;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
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

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let upload_limit = state.config.max_upload_bytes;

    let api = Router::new()
        .route("/info", get(info))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
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
        .route("/posts/{id}/restore", post(posts::restore))
        .route("/posts/{id}/vote", put(posts::vote))
        .route("/tags", get(tags::list))
        .route("/tags/{name}", axum::routing::patch(tags::update))
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

    app.layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::custom(panic_response))
        .with_state(state)
}

async fn info(State(state): State<AppState>) -> AppResult<Json<ServerInfo>> {
    let has_users: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&state.db)
        .await?;
    Ok(Json(ServerInfo {
        name: state.config.instance_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        has_users,
        registration_open: !has_users || state.config.allow_registration,
        max_upload_bytes: state.config.max_upload_bytes as u64,
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
