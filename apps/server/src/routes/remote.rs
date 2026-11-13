//! `POST /remote/search`: a page of a search on another booru, fetched by
//! the server like a URL import (same address rules) and turned into
//! StarBooru's terms. Importing a result is a plain `POST /posts/import`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use starbooru_core::remote::{REMOTE_PAGE_SIZE, clean_base};
use starbooru_core::{
    BooruAccount, MAX_BOORU_ACCOUNTS, RemoteError, RemotePage, RemoteSearch, SaveBooruAccount,
};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::fetch::Answer;
use crate::remote::{self, accounts, pace};
use crate::state::AppState;

/// `GET /remote/accounts`: the caller's saved accounts on other boorus,
/// without their keys.
pub async fn accounts(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<BooruAccount>>> {
    accounts::list(&state, user.id).await.map(Json)
}

/// `PUT /remote/accounts`: saves the account for a site, sealed, in place of
/// the one saved for that site before.
pub async fn save_account(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<SaveBooruAccount>,
) -> AppResult<Json<BooruAccount>> {
    let req = req
        .clean()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let others: i64 =
        sqlx::query_scalar("SELECT count(*) FROM booru_accounts WHERE user_id = $1 AND base <> $2")
            .bind(user.id)
            .bind(&req.base)
            .fetch_one(&state.db)
            .await?;
    if usize::try_from(others).unwrap_or(usize::MAX) >= MAX_BOORU_ACCOUNTS {
        return Err(AppError::Conflict {
            code: "too_many_accounts",
            message: format!("at most {MAX_BOORU_ACCOUNTS} accounts can be saved"),
        });
    }
    let sealed = state.vault.seal(user.id, &req.base, &req.api_key)?;
    let (id, updated_at): (i64, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO booru_accounts (user_id, api, base, login, sealed_key) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (user_id, base) DO UPDATE SET api = EXCLUDED.api, login = EXCLUDED.login, \
             sealed_key = EXCLUDED.sealed_key, updated_at = now() \
         RETURNING id, updated_at",
    )
    .bind(user.id)
    .bind(req.api.as_str())
    .bind(&req.base)
    .bind(&req.login)
    .bind(sealed)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(BooruAccount {
        id,
        api: req.api,
        base: req.base,
        login: req.login,
        usable: true,
        updated_at,
    }))
}

/// `DELETE /remote/accounts/{id}`: forgets a saved account.
pub async fn delete_account(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let gone = sqlx::query("DELETE FROM booru_accounts WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if gone.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<RemoteSearch>,
) -> AppResult<Json<RemotePage>> {
    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("URL import is turned off on this server".into()))?;
    // A search that brings no account of its own uses the one saved for the
    // site, before the limits are checked: they depend on having one.
    let mut req = req;
    let blank = |part: &Option<String>| part.as_deref().is_none_or(|p| p.trim().is_empty());
    if (blank(&req.login) || blank(&req.api_key))
        && let Ok(base) = clean_base(&req.base)
        && let Some((login, key)) = accounts::load(&state, user.id).await?.of(&base)
    {
        req.login = Some(login.to_owned());
        req.api_key = Some(key.to_owned());
    }
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
    async fn a_saved_account_is_used_and_never_shown() {
        let Some(app) = testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Any)).await
        else {
            return;
        };
        let t = app.register("admin").await;
        let other = app.register("other").await;
        let site = fake_booru().await;
        let base = format!("{site}/d");
        let body = json!({"api": "danbooru", "base": base, "tags": "x"});
        let search = |token: String| {
            let body = body.clone();
            let app = &app;
            async move {
                app.call(
                    Method::POST,
                    "/api/v1/remote/search",
                    Some(&token),
                    Some(body),
                )
                .await
            }
        };
        // Without an account the fake site gives three posts; with one, forty.
        assert_eq!(
            search(t.clone()).await.json()["posts"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );

        let save = |token: String, login: &str, key: &str| {
            let account = json!({"api": "danbooru", "base": format!("{base}/"), "login": login, "api_key": key});
            let app = &app;
            async move {
                app.call(
                    Method::PUT,
                    "/api/v1/remote/accounts",
                    Some(&token),
                    Some(account),
                )
                .await
            }
        };
        let r = save(t.clone(), "", "secret").await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = save(t.clone(), "me", "wrong").await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        // Saved again for the same site: replaced, not added.
        let r = save(t.clone(), " me ", " secret ").await;
        let saved = r.json();
        assert_eq!(saved["base"], json!(base));
        assert_eq!(
            (saved["login"].as_str(), saved["usable"].as_bool()),
            (Some("me"), Some(true))
        );
        assert!(!saved.to_string().contains("secret"));

        let list = app.get("/api/v1/remote/accounts", &t).await.json();
        assert_eq!(list.as_array().map(Vec::len), Some(1));
        assert_eq!(list[0]["id"], saved["id"]);
        assert!(!list.to_string().contains("secret"), "{list}");
        // What the database holds doesn't contain the key either.
        let sealed: Vec<u8> = sqlx::query_scalar("SELECT sealed_key FROM booru_accounts")
            .fetch_one(app.db())
            .await
            .unwrap();
        assert!(!sealed.windows(6).any(|w| w == b"secret"));

        // The search needs no key from the app now, and each account has its own.
        let r = search(t.clone()).await;
        assert_eq!(r.json()["posts"].as_array().map(Vec::len), Some(40));
        assert!(!r.json().to_string().contains("secret"));
        assert_eq!(
            search(other.clone()).await.json()["posts"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        assert_eq!(
            app.get("/api/v1/remote/accounts", &other).await.json(),
            json!([])
        );

        // Only its owner forgets it.
        let path = format!("/api/v1/remote/accounts/{}", saved["id"]);
        let r = app.call(Method::DELETE, &path, Some(&other), None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app.call(Method::DELETE, &path, Some(&t), None).await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        assert_eq!(
            search(t.clone()).await.json()["posts"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        let r = app
            .call(Method::GET, "/api/v1/remote/accounts", None, None)
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        app.finish().await;
    }

    #[tokio::test]
    async fn an_account_sealed_with_another_secret_is_listed_as_unusable() {
        let Some(app) = testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Any)).await
        else {
            return;
        };
        let t = app.register("admin").await;
        let account =
            json!({"api": "gelbooru", "base": "https://g.example", "login": "42", "api_key": "k"});
        app.call(
            Method::PUT,
            "/api/v1/remote/accounts",
            Some(&t),
            Some(account),
        )
        .await;
        // As after STARBOORU_SECRET changed.
        let elsewhere =
            crate::remote::accounts::Vault::new(b"another-secret-another-secret-another");
        let sealed = elsewhere.seal(1, "https://g.example", "k").unwrap();
        sqlx::query("UPDATE booru_accounts SET sealed_key = $1")
            .bind(sealed)
            .execute(app.db())
            .await
            .unwrap();
        let list = app.get("/api/v1/remote/accounts", &t).await.json();
        assert_eq!(list[0]["usable"], false);
        assert_eq!(list[0]["login"], "42");
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
