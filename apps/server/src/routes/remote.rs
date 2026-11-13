//! `POST /remote/search`: a page of a search on another booru, fetched by
//! the server like a URL import (same address rules) and turned into
//! StarBooru's terms. Importing a result is a plain `POST /posts/import`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use starbooru_core::remote::{REMOTE_PAGE_SIZE, clean_base};
use starbooru_core::{
    BooruAccount, BooruApi, MAX_BOORU_ACCOUNTS, MAX_REMOTE_SEARCHES, RemoteError, RemotePage,
    RemotePost, RemotePostQuery, RemoteSearch, RemoteSite, RemoteTag, RemoteTagQuery,
    SaveBooruAccount, SaveRemoteSearch, SavedRemoteSearch,
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
    // A Gelbooru known to want a key isn't asked again without one: its
    // website lists the same search.
    let keyless = req.api == BooruApi::Gelbooru && req.api_key.is_none();
    let site = pace::api_key(&base);
    if keyless && state.pace.locked(&site, Utc::now()) {
        return listed(&state, user.id, &req).await;
    }
    let answer = pace::ask(&state.pace, fetcher, req.api, &url).await?;
    if !answer.is_success() {
        if matches!(answer.status, 401 | 403) && req.api_key.is_none() {
            state.pace.lock(&site, Utc::now());
            if keyless {
                return listed(&state, user.id, &req).await;
            }
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
    let posts = finished(&state, posts).await?;
    Ok(Json(RemotePage { posts, more }))
}

/// A Gelbooru search read from the site's own list pages, for when its API
/// wants a key the user hasn't got.
async fn listed(state: &AppState, _user: i64, req: &RemoteSearch) -> AppResult<Json<RemotePage>> {
    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("URL import is turned off on this server".into()))?;
    let url = remote::list_url(req)
        .ok_or_else(|| AppError::BadRequest(RemoteError::BadBase.to_string()))?;
    let answer = pace::ask(&state.pace, fetcher, req.api, &url).await?;
    if !answer.is_success() {
        return Err(refusal(&answer));
    }
    let posts = remote::html::read_list(&url, &answer.body);
    let full = posts.len() >= usize::try_from(remote::LIST_PAGE_SIZE).unwrap_or(usize::MAX);
    let more = full && req.page < req.api.limits().max_page;
    let posts = finished(state, posts).await?;
    Ok(Json(RemotePage { posts, more }))
}

/// Search results as they go out: each marked with the post it became here,
/// if it was imported, and with its pictures behind this server where its
/// site wouldn't show them to a page of ours.
async fn finished(state: &AppState, mut posts: Vec<RemotePost>) -> AppResult<Vec<RemotePost>> {
    let pages: Vec<&str> = posts.iter().map(|p| p.page_url.as_str()).collect();
    let known: Vec<(String, i64)> = sqlx::query_as(
        "SELECT source, min(id) FROM posts WHERE source = ANY($1) AND deleted_at IS NULL \
         GROUP BY source",
    )
    .bind(&pages)
    .fetch_all(&state.db)
    .await?;
    let now = Utc::now().timestamp();
    for post in &mut posts {
        post.post_id = known
            .iter()
            .find(|(source, _)| *source == post.page_url)
            .map(|(_, id)| *id);
        state.relay.guard(post, now);
    }
    Ok(posts)
}

/// `POST /remote/tags`: tags on a site that start with what was typed, for
/// the search box. A site with nothing to say gives none; that isn't an error.
pub async fn tags(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(req): Json<RemoteTagQuery>,
) -> AppResult<Json<Vec<RemoteTag>>> {
    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("URL import is turned off on this server".into()))?;
    let base = clean_base(&req.base).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let term = req.term.trim();
    if term.is_empty() || term.len() > 128 || term.chars().any(|c| c.is_control()) {
        return Ok(Json(Vec::new()));
    }
    let url = remote::base_url(&base).and_then(|base| remote::tags_url(req.api, &base, term));
    let Some(url) = url else {
        return Ok(Json(Vec::new()));
    };
    // A site that is slow or down has no suggestions; only "busy" is passed
    // on, so the app stops asking for a while.
    let answer = match pace::ask(&state.pace, fetcher, req.api, &url).await {
        Ok(answer) if answer.is_success() => answer,
        Err(busy @ AppError::TooManyRequests { .. }) => return Err(busy),
        _ => return Ok(Json(Vec::new())),
    };
    Ok(Json(remote::parse_tags(req.api, &answer.body)))
}

/// `POST /remote/post`: one post in full, from a link to it.
pub async fn post(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<RemotePostQuery>,
) -> AppResult<Json<RemotePost>> {
    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("URL import is turned off on this server".into()))?;
    let accounts = accounts::load(&state, user.id).await?;
    let found = remote::lookup::post(&state.pace, fetcher, &accounts, &req.url)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut posts = finished(&state, vec![found]).await?;
    posts.pop().map(Json).ok_or(AppError::NotFound)
}

#[derive(FromRow)]
struct SearchRow {
    id: i64,
    name: String,
    query: String,
    sites: String,
    created_at: DateTime<Utc>,
}

impl From<SearchRow> for SavedRemoteSearch {
    fn from(row: SearchRow) -> Self {
        SavedRemoteSearch {
            id: row.id,
            name: row.name,
            query: row.query,
            // Sites of a kind this version doesn't know are left out.
            sites: serde_json::from_str::<Vec<serde_json::Value>>(&row.sites)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|site| serde_json::from_value::<RemoteSite>(site).ok())
                .collect(),
            created_at: row.created_at,
        }
    }
}

/// `GET /remote/searches`: the caller's saved searches on other sites, by name.
pub async fn searches(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<SavedRemoteSearch>>> {
    let rows: Vec<SearchRow> = sqlx::query_as(
        "SELECT id, name, query, sites, created_at FROM remote_searches WHERE user_id = $1 \
         ORDER BY lower(name), id",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// `POST /remote/searches`: keeps a search and the sites it goes to.
pub async fn save_search(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<SaveRemoteSearch>,
) -> AppResult<Json<SavedRemoteSearch>> {
    let req = req
        .clean()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_searches WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&state.db)
        .await?;
    if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_REMOTE_SEARCHES {
        return Err(AppError::Conflict {
            code: "too_many_searches",
            message: format!("at most {MAX_REMOTE_SEARCHES} searches can be saved"),
        });
    }
    let sites = serde_json::to_string(&req.sites).map_err(AppError::internal)?;
    let row: SearchRow = sqlx::query_as(
        "INSERT INTO remote_searches (user_id, name, query, sites) VALUES ($1, $2, $3, $4) \
         RETURNING id, name, query, sites, created_at",
    )
    .bind(user.id)
    .bind(&req.name)
    .bind(&req.query)
    .bind(sites)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row.into()))
}

/// `DELETE /remote/searches/{id}`.
pub async fn delete_search(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let gone = sqlx::query("DELETE FROM remote_searches WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if gone.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
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
            .route("/html/posts.json", get(|| async { "<html>hi</html>" }))
            .route(
                "/d/autocomplete.json",
                get(|| async {
                    json!([{"type": "tag-word", "label": "blue sky", "value": "blue_sky",
                            "category": 0, "post_count": 12},
                           {"type": "tag-word", "label": "x", "value": "blue_archive",
                            "category": 3, "post_count": 5}])
                    .to_string()
                }),
            )
            .route("/d/a.png", get(|| async { testing::png(8, 8, 2) }))
            .route("/d/page.html", get(|| async { "<!DOCTYPE html><html></html>" }))
            // A Gelbooru that wants a key for its API, and lists posts on its pages.
            .route(
                "/k/index.php",
                get(|RawQuery(q): RawQuery| async move {
                    let q = q.unwrap_or_default();
                    if q.contains("page=dapi") {
                        return (StatusCode::UNAUTHORIZED, String::new());
                    }
                    let item = |id: u32| {
                        format!(
                            r#"<article class="thumbnail-preview"><a id="p{id}" href="index.php?page=post&amp;s=view&amp;id={id}">
                            <img src="/thumbnails/{id}.jpg" title=" sky t{id} score:{id} rating:general"/></a></article>"#
                        )
                    };
                    let from: u32 = q
                        .split('&')
                        .find_map(|pair| pair.strip_prefix("pid="))
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0);
                    // 42 on the first page, 3 on the next.
                    let count = if from == 0 { 42 } else { 3 };
                    let items: String = (1..=count).map(|i| item(from + i)).collect();
                    (StatusCode::OK, format!("<!DOCTYPE html><html><body>{items}</body></html>"))
                }),
            );
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
    async fn gelbooru_is_listed_from_its_pages_without_a_key() {
        let Some(app) = testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Any)).await
        else {
            return;
        };
        let t = app.register("admin").await;
        let site = fake_booru().await;
        let search = |page: u32| {
            let body = json!({"api": "gelbooru", "base": format!("{site}/k"), "tags": "sky", "page": page});
            app.call(Method::POST, "/api/v1/remote/search", Some(&t), Some(body))
        };
        let r = search(1).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let page = r.json();
        assert_eq!(page["posts"].as_array().map(Vec::len), Some(42));
        assert_eq!(page["more"], true);
        let first = &page["posts"][0];
        assert_eq!(first["id"], 1);
        assert_eq!(first["tags"], json!(["sky", "t1"]));
        assert_eq!(
            (first["rating"].as_str(), first["score"].as_i64()),
            (Some("safe"), Some(1))
        );
        assert_eq!(
            first["page_url"],
            format!("{site}/k/index.php?page=post&s=view&id=1")
        );
        // A list doesn't say where the file is: an import goes by the page.
        assert_eq!(first["file_url"], "");
        let r = search(2).await;
        assert_eq!(r.json()["posts"].as_array().map(Vec::len), Some(3));
        assert_eq!(r.json()["posts"][0]["id"], 43);
        assert_eq!(r.json()["more"], false);
        app.finish().await;
    }

    #[tokio::test]
    async fn suggests_tags_marks_imported_posts_and_keeps_searches() {
        let Some(app) = testing::app_with(|c| {
            c.url_import = Some(crate::fetch::Reach::Any);
            c.max_upload_bytes = 1 << 20;
        })
        .await
        else {
            return;
        };
        let t = app.register("admin").await;
        let other = app.register("other").await;
        let site = fake_booru().await;
        let base = format!("{site}/d");

        let tags = |term: &str| {
            let body = json!({"api": "danbooru", "base": base, "term": term});
            app.call(Method::POST, "/api/v1/remote/tags", Some(&t), Some(body))
        };
        let found = tags("blu").await.json();
        assert_eq!(
            found[0],
            json!({"name": "blue_sky", "category": "general", "count": 12})
        );
        assert_eq!(found[1]["category"], "copyright");
        // Nothing typed, or a site without suggestions: none, not an error.
        assert_eq!(tags("  ").await.json(), json!([]));
        let nowhere = json!({"api": "moebooru", "base": format!("{site}/missing"), "term": "blu"});
        let r = app
            .call(Method::POST, "/api/v1/remote/tags", Some(&t), Some(nowhere))
            .await;
        assert_eq!((r.status, r.json()), (StatusCode::OK, json!([])));

        // A result imported before says which post it is here.
        let search = json!({"api": "danbooru", "base": base, "tags": "x"});
        let import = json!({"url": format!("{base}/a.png"), "source": format!("{base}/posts/2")});
        let made = app
            .call(Method::POST, "/api/v1/posts/import", Some(&t), Some(import))
            .await;
        assert_eq!(made.status, StatusCode::OK, "{:?}", made.json());
        let r = app
            .call(
                Method::POST,
                "/api/v1/remote/search",
                Some(&t),
                Some(search),
            )
            .await;
        let posts = r.json()["posts"].clone();
        assert!(posts[0].get("post_id").is_none());
        assert_eq!(posts[1]["post_id"], made.json()["id"]);

        // Saved searches: kept with their sites, each account its own.
        let save = |name: &str, sites: serde_json::Value| {
            let body = json!({"name": name, "query": " blue_sky  rating:safe ", "sites": sites});
            app.call(
                Method::POST,
                "/api/v1/remote/searches",
                Some(&t),
                Some(body),
            )
        };
        let sites = json!([{"api": "danbooru", "base": "https://danbooru.donmai.us/"},
                           {"api": "moebooru", "base": "https://yande.re"}]);
        let r = save("Skies", sites).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let kept = r.json();
        assert_eq!(kept["query"], "blue_sky rating:safe");
        assert_eq!(kept["sites"][0]["base"], "https://danbooru.donmai.us");
        assert_eq!(
            save("None", json!([])).await.status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            save(" ", json!([{"api": "e621", "base": "https://e926.net"}]))
                .await
                .status,
            StatusCode::BAD_REQUEST
        );
        let list = app.get("/api/v1/remote/searches", &t).await.json();
        assert_eq!(list.as_array().map(Vec::len), Some(1));
        assert_eq!(list[0]["sites"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            app.get("/api/v1/remote/searches", &other).await.json(),
            json!([])
        );
        let path = format!("/api/v1/remote/searches/{}", kept["id"]);
        let r = app.call(Method::DELETE, &path, Some(&other), None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
        let r = app.call(Method::DELETE, &path, Some(&t), None).await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        assert_eq!(
            app.get("/api/v1/remote/searches", &t).await.json(),
            json!([])
        );
        app.finish().await;
    }

    #[tokio::test]
    async fn passes_on_only_pictures_it_signed_for() {
        let Some(app) = testing::app_with(|c| c.url_import = Some(crate::fetch::Reach::Any)).await
        else {
            return;
        };
        let site = fake_booru().await;
        let now = chrono::Utc::now().timestamp();
        let signed = |path: &str| app.state.relay.url(&format!("{site}{path}"), now);
        // No sign-in: the address is the permission, as for `/media`.
        let r = app.call(Method::GET, &signed("/d/a.png"), None, None).await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.headers["content-type"], "image/png");
        assert_eq!(r.body, testing::png(8, 8, 2));
        // Signed or not, a page isn't passed on as a picture.
        let r = app
            .call(Method::GET, &signed("/d/page.html"), None, None)
            .await;
        assert_eq!(r.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        // An address nobody signed, or one changed afterwards.
        let forged = signed("/d/a.png").replace("a.png", "page.html");
        let r = app.call(Method::GET, &forged, None, None).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        let bare = format!(
            "/api/v1/remote/media?u={site}/d/a.png&exp={}&sig=00",
            now + 60
        );
        let r = app.call(Method::GET, &bare, None, None).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
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
