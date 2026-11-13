//! `POST /remote/search`: a page of a search on another booru, fetched by
//! the server like a URL import (same address rules) and turned into
//! StarBooru's terms. Importing a result is a plain `POST /posts/import`.

use axum::Json;
use axum::extract::State;
use starbooru_core::remote::REMOTE_PAGE_SIZE;
use starbooru_core::{RemoteError, RemotePage, RemoteSearch};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::fetch::FetchError;
use crate::remote;
use crate::state::AppState;

/// Largest search answer read; a page of 40 posts is well under 1 MiB.
const MAX_ANSWER: usize = 8 * 1024 * 1024;
/// How long to wait after a site says it's busy, when it doesn't say.
const BUSY_WAIT_SECS: u64 = 5;

pub async fn search(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(req): Json<RemoteSearch>,
) -> AppResult<Json<RemotePage>> {
    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("URL import is turned off on this server".into()))?;
    let req = req
        .clean()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let bad_base = || AppError::BadRequest(RemoteError::BadBase.to_string());
    let base = remote::base_url(&req.base).ok_or_else(bad_base)?;
    let url = remote::search_url(&req).ok_or_else(bad_base)?;
    // The URL may carry the user's API key: never put it in an error or log.
    let body = fetcher
        .get(url.as_str(), MAX_ANSWER)
        .await
        .map_err(site_error)?;
    let (posts, sent) = remote::parse(req.api, &base, &body).ok_or_else(|| {
        AppError::Upstream(format!(
            "that site's answer isn't a {} search; check the address and the API",
            req.api.name()
        ))
    })?;
    let more = sent >= usize::try_from(REMOTE_PAGE_SIZE).unwrap_or(usize::MAX);
    Ok(Json(RemotePage { posts, more }))
}

/// What went wrong, in terms of what the user can change.
fn site_error(e: FetchError) -> AppError {
    match e {
        FetchError::BadUrl | FetchError::Blocked => AppError::BadRequest(e.to_string()),
        FetchError::Status(401 | 403) => AppError::BadRequest(
            "that site wants an account: add your login (or user ID) and API key".into(),
        ),
        FetchError::Status(400 | 422) => AppError::BadRequest(
            "that site refused the search; check its syntax and limits (Danbooru allows two tags without an account)".into(),
        ),
        FetchError::Status(429 | 503) => AppError::TooManyRequests {
            retry_after: BUSY_WAIT_SECS,
        },
        FetchError::TooLarge => AppError::Upstream("that site's answer is too large".into()),
        _ => AppError::Upstream(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    use crate::testing;

    /// A fake site with one endpoint per API, plus failures.
    async fn fake_booru() -> String {
        use axum::extract::RawQuery;
        use axum::routing::get;
        let app = axum::Router::new()
            .route(
                "/d/posts.json",
                get(|RawQuery(q): RawQuery| async move {
                    let q = q.unwrap_or_default();
                    if q.contains("tags=a+b+c") {
                        return (StatusCode::UNPROCESSABLE_ENTITY, String::new());
                    }
                    let posts: Vec<_> = (1..=40)
                        .map(|i| {
                            json!({"id": i, "file_url": format!("https://cdn.example/{i}.jpg"),
                            "tag_string": format!("t{i} -_-"), "rating": "g",
                            "image_width": 10, "image_height": 10})
                        })
                        .collect();
                    // The key and login arrive as Danbooru expects them.
                    let signed = q.contains("login=me") && q.contains("api_key=secret");
                    let posts = if signed { posts } else { posts[..3].to_vec() };
                    (StatusCode::OK, json!(posts).to_string())
                }),
            )
            .route(
                "/g/index.php",
                get(|| async { (StatusCode::UNAUTHORIZED, "") }),
            )
            .route(
                "/busy/post.json",
                get(|| async { (StatusCode::TOO_MANY_REQUESTS, "") }),
            )
            .route("/html/posts.json", get(|| async { "<html>hi</html>" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn searches_another_booru() {
        let Some(app) = testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Any)).await
        else {
            return;
        };
        let t = app.register("admin").await;
        let site = fake_booru().await;
        let search = |api: &str, path: &str, extra: serde_json::Value| {
            let mut body = json!({"api": api, "base": format!("{site}/{path}"), "tags": "x"});
            if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
                b.extend(e.clone());
            }
            body
        };

        let r = app
            .call(
                Method::POST,
                "/api/v1/remote/search",
                Some(&t),
                Some(search("danbooru", "d", json!({}))),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let page = r.json();
        assert_eq!(page["more"], false);
        assert_eq!(page["posts"][0]["tags"], json!(["t1"]));
        assert_eq!(page["posts"][0]["page_url"], format!("{site}/d/posts/1"));
        assert_eq!(page["posts"][0]["rating"], "safe");

        let signed = search("danbooru", "d", json!({"login": "me", "api_key": "secret"}));
        let r = app
            .call(
                Method::POST,
                "/api/v1/remote/search",
                Some(&t),
                Some(signed),
            )
            .await;
        assert_eq!(
            (r.status, r.json()["more"].clone()),
            (StatusCode::OK, json!(true))
        );
        assert_eq!(r.json()["posts"].as_array().map(Vec::len), Some(40));

        for (body, status, code) in [
            (
                search("danbooru", "d", json!({"tags": "a b c"})),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
            (
                search("gelbooru", "g", json!({})),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
            (
                search("moebooru", "busy", json!({})),
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
            ),
            (
                search("e621", "html", json!({})),
                StatusCode::FAILED_DEPENDENCY,
                "fetch_failed",
            ),
            (
                search("e621", "missing", json!({})),
                StatusCode::FAILED_DEPENDENCY,
                "fetch_failed",
            ),
            (
                json!({"api": "danbooru", "base": "ftp://x"}),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
            (
                json!({"api": "danbooru", "base": site, "page": 0}),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
        ] {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/remote/search",
                    Some(&t),
                    Some(body.clone()),
                )
                .await;
            assert_eq!(
                (r.status, r.json()["code"].as_str()),
                (status, Some(code)),
                "{body}"
            );
            // An account's key never comes back in an error.
            assert!(!r.json().to_string().contains("secret"));
        }

        let r = app
            .call(
                Method::POST,
                "/api/v1/remote/search",
                None,
                Some(search("danbooru", "d", json!({}))),
            )
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        app.finish().await;
    }

    #[tokio::test]
    async fn refuses_private_sites_by_default() {
        let Some(app) =
            testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Public)).await
        else {
            return;
        };
        let t = app.register("admin").await;
        let body = json!({"api": "danbooru", "base": "http://127.0.0.1:9"});
        let r = app
            .call(Method::POST, "/api/v1/remote/search", Some(&t), Some(body))
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        app.finish().await;
    }
}
