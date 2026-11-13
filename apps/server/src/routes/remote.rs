//! `POST /remote/search`: a page of a search on another booru, fetched by
//! the server like a URL import (same address rules) and turned into
//! StarBooru's terms. Importing a result is a plain `POST /posts/import`.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use starbooru_core::remote::REMOTE_PAGE_SIZE;
use starbooru_core::{RemoteError, RemotePage, RemoteSearch};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::fetch::Answer;
use crate::remote::{self, pace};
use crate::state::AppState;

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
    let answer = pace::ask(&state.pace, fetcher, req.api, &url).await?;
    if !answer.is_success() {
        if matches!(answer.status, 401 | 403) && req.api_key.is_none() {
            state.pace.lock(&pace::site_key(&url), Utc::now());
        }
        return Err(refusal(&answer));
    }
    let (posts, sent) = remote::parse(req.api, &base, &answer.body).ok_or_else(|| {
        // Gelbooru's forks say what went wrong under a plain 200.
        AppError::Upstream(match pace::site_message(&answer.body) {
            Some(said) => format!("that site says: {said}"),
            None => format!(
                "that site's answer isn't a {} search; check the address and the API",
                req.api.name()
            ),
        })
    })?;
    let full = sent >= usize::try_from(REMOTE_PAGE_SIZE).unwrap_or(usize::MAX);
    let more = full && req.page < req.api.limits().max_page;
    Ok(Json(RemotePage { posts, more }))
}

/// What a site's refusal means for the user, in the site's own words when it
/// gave any: they name the limit that was hit.
pub(crate) fn refusal(answer: &Answer) -> AppError {
    let said = pace::site_message(&answer.body).map(|said| format!("that site says: {said}"));
    match (answer.status, said) {
        (401 | 403, _) => AppError::BadRequest(
            "that site wants an account: add your login (or user ID) and API key".into(),
        ),
        (400..=499, Some(said)) => AppError::BadRequest(said),
        (400 | 422, None) => {
            AppError::BadRequest("that site refused the search; check its syntax and limits".into())
        }
        (_, Some(said)) => AppError::Upstream(said),
        (status, None) => AppError::Upstream(format!("that address answered with HTTP {status}")),
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
                get(|| async {
                    let mut headers = axum::http::HeaderMap::new();
                    headers.insert(axum::http::header::RETRY_AFTER, "42".parse().unwrap());
                    (StatusCode::TOO_MANY_REQUESTS, headers, "")
                }),
            )
            .route(
                "/said/posts.json",
                get(|| async {
                    (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        r#"{"success":false,"message":"You cannot search for more than 2 tags at a time."}"#,
                    )
                }),
            )
            .route(
                "/down/posts.json",
                get(|| async {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        r#"{"success":false,"message":"The database timed out running your query."}"#,
                    )
                }),
            )
            .route(
                "/xml/index.php",
                get(|| async {
                    r#"<?xml version="1.0" encoding="UTF-8"?><response success="false" reason="Search error: API limited due to abuse."/>"#
                }),
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

        for (body, status, code, says) in [
            (
                search("danbooru", "d", json!({"tags": "a b c"})),
                StatusCode::BAD_REQUEST,
                "bad_request",
                "takes 2 tags",
            ),
            (
                search("gelbooru", "g", json!({})),
                StatusCode::BAD_REQUEST,
                "bad_request",
                "wants an account",
            ),
            // What a site says about its limits is passed on.
            (
                search(
                    "danbooru",
                    "said",
                    json!({"login": "me", "api_key": "secret"}),
                ),
                StatusCode::BAD_REQUEST,
                "bad_request",
                "that site says: You cannot search for more than 2 tags at a time.",
            ),
            (
                search("danbooru", "down", json!({})),
                StatusCode::FAILED_DEPENDENCY,
                "fetch_failed",
                "that site says: The database timed out running your query.",
            ),
            (
                search("gelbooru", "xml", json!({})),
                StatusCode::FAILED_DEPENDENCY,
                "fetch_failed",
                "that site says: Search error: API limited due to abuse.",
            ),
            (
                search("e621", "html", json!({})),
                StatusCode::FAILED_DEPENDENCY,
                "fetch_failed",
                "isn't a e621 search",
            ),
            (
                search("e621", "missing", json!({})),
                StatusCode::FAILED_DEPENDENCY,
                "fetch_failed",
                "HTTP 404",
            ),
            (
                json!({"api": "danbooru", "base": "ftp://x"}),
                StatusCode::BAD_REQUEST,
                "bad_request",
                "http or https",
            ),
            (
                json!({"api": "danbooru", "base": site, "page": 0}),
                StatusCode::BAD_REQUEST,
                "bad_request",
                "pages go from",
            ),
            (
                json!({"api": "e621", "base": site, "page": 751}),
                StatusCode::BAD_REQUEST,
                "bad_request",
                "past page 750",
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
            let message = r.json()["message"].as_str().unwrap_or_default().to_owned();
            assert!(message.contains(says), "{body}: {message}");
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
    async fn a_busy_site_is_left_alone_for_as_long_as_it_asks() {
        let Some(app) = testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Any)).await
        else {
            return;
        };
        let t = app.register("admin").await;
        let site = fake_booru().await;
        let search = |path: &str| {
            let body = json!({"api": "moebooru", "base": format!("{site}/{path}")});
            app.call(Method::POST, "/api/v1/remote/search", Some(&t), Some(body))
        };
        let r = search("busy").await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(r.headers["retry-after"], "42");
        // Nothing else goes to that site meanwhile, not even what would work.
        let started = std::time::Instant::now();
        let r = search("d").await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
        let wait: u64 = r.headers["retry-after"].to_str().unwrap().parse().unwrap();
        assert!((40..=42).contains(&wait), "{wait}");
        assert!(started.elapsed().as_secs() < 2, "it answers at once");
        app.finish().await;
    }

    #[tokio::test]
    async fn requests_to_one_site_are_spaced_out() {
        let Some(app) = testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Any)).await
        else {
            return;
        };
        let t = app.register("admin").await;
        let site = fake_booru().await;
        let body = json!({"api": "danbooru", "base": format!("{site}/d")});
        let started = std::time::Instant::now();
        for _ in 0..3 {
            let r = app
                .call(
                    Method::POST,
                    "/api/v1/remote/search",
                    Some(&t),
                    Some(body.clone()),
                )
                .await;
            assert_eq!(r.status, StatusCode::OK);
        }
        // Three requests, two gaps of Danbooru's half second.
        let took = started.elapsed().as_millis();
        assert!(took >= 900, "three searches took {took} ms");
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
