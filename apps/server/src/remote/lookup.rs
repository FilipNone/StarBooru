//! Finding the post behind an address someone pasted or dropped: a post's
//! page on a booru, or one of its files. The site's API is asked first, as it
//! knows the original file, the tags and the rating. A page the API can't
//! answer for is read as a page (`html`).

use chrono::{TimeDelta, Utc};
use starbooru_core::tag::MAX_TAGS_PER_POST;
use starbooru_core::{ImportRequest, RemotePost, post_link};
use url::Url;

use super::pace::{self, BUSY_WAIT_SECS, Pace};
use super::{base_url, html, parse, post_url};
use crate::error::{AppError, AppResult};
use crate::fetch::{FetchError, Fetcher, Watch};

/// What asking a site about an address came to.
pub enum Lookup {
    Found(Box<RemotePost>),
    /// Not a post's address, or the site has no such post.
    Unknown,
    /// It is one, but the site's API wouldn't say; with its status, when it
    /// answered at all.
    Refused(Option<u16>),
}

/// Asks the site an address belongs to which post it is, in its turn. A site
/// that says it is busy is an error that says when to come back.
pub async fn find(pace: &Pace, fetcher: &Fetcher, raw: &str) -> AppResult<Lookup> {
    let Some(link) = post_link(raw, &[]) else {
        return Ok(Lookup::Unknown);
    };
    let url = base_url(&link.base)
        .and_then(|base| Some((post_url(link.api, &base, &link.post, None)?, base)));
    let Some((url, base)) = url else {
        return Ok(Lookup::Unknown);
    };
    let host = pace::site_key(&url);
    // No use asking again what it refused a moment ago.
    if pace.locked(&host, Utc::now()) {
        return Ok(Lookup::Refused(Some(401)));
    }
    let answer = match pace::ask(pace, fetcher, link.api, &url).await {
        Ok(answer) => answer,
        Err(busy @ AppError::TooManyRequests { .. }) => return Err(busy),
        Err(_) => return Ok(Lookup::Refused(None)),
    };
    if !answer.is_success() {
        if matches!(answer.status, 401 | 403) {
            pace.lock(&host, Utc::now());
        }
        return Ok(Lookup::Refused(Some(answer.status)));
    }
    Ok(parse(link.api, &base, &answer.body)
        .and_then(|(posts, _)| posts.into_iter().next())
        .map_or(Lookup::Unknown, |post| Lookup::Found(Box::new(post))))
}

/// `own` tags followed by `more`, without repeats, as many as a post takes.
fn merged(own: &[String], more: Vec<String>) -> Vec<String> {
    let mut tags = own.to_vec();
    for tag in more {
        if tags.len() >= MAX_TAGS_PER_POST {
            break;
        }
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

/// An import with what its site says about the post filled in: the original
/// file's address, the site's tags after the user's own, its rating, and the
/// post's page as the source.
fn filled(req: &ImportRequest, post: RemotePost) -> ImportRequest {
    ImportRequest {
        tags: merged(&req.tags, post.import_tags()),
        rating: post.rating,
        source: Some(post.page_url),
        url: post.file_url,
    }
}

/// Whether an import still needs its post looked up: one that comes with a
/// source of its own (a search result, say) already knows its post.
pub fn wants_lookup(req: &ImportRequest) -> bool {
    req.source
        .as_deref()
        .is_none_or(|source| source.trim() == req.url.trim())
}

/// Downloads what an import asks for. An address that is a post on a booru
/// becomes that post's original file, tags and rating; any other web page
/// becomes the picture it is about. Returns the file and the import as it
/// ended up. `watch` hears how far each download has got. A site that says
/// it is busy is an error that says when to come back, and nothing else goes
/// to it meanwhile.
pub async fn fetch(
    fetcher: &Fetcher,
    pace: &Pace,
    req: ImportRequest,
    max_bytes: usize,
    watch: Watch<'_>,
) -> AppResult<(Vec<u8>, ImportRequest)> {
    let mut req = req;
    let mut refused = None;
    if wants_lookup(&req) {
        match find(pace, fetcher, &req.url).await? {
            Lookup::Found(post) => req = filled(&req, *post),
            Lookup::Unknown => {}
            Lookup::Refused(status) => refused = Some(status),
        }
    }
    // The page a file belongs to, which some sites want named.
    let from = req.source.clone();
    let bytes = download(fetcher, pace, &req.url, from.as_deref(), max_bytes, watch).await?;
    if !html::is_html(&bytes) {
        return Ok((bytes, req));
    }

    // A page, not a file: take the picture it names.
    let page = Url::parse(req.url.trim())
        .ok()
        .and_then(|address| html::read(&address, &bytes))
        .ok_or_else(|| no_picture(refused))?;
    let Some(file_url) = page.file_url else {
        return Err(no_picture(refused));
    };
    let page_url = req.url.clone();
    let req = ImportRequest {
        tags: merged(&req.tags, page.groups.import_tags(&page.tags)),
        rating: page.rating.unwrap_or(req.rating),
        source: Some(req.source.unwrap_or_else(|| page_url.clone())),
        url: file_url,
    };
    let bytes = download(fetcher, pace, &req.url, Some(&page_url), max_bytes, watch).await?;
    if html::is_html(&bytes) {
        return Err(no_picture(refused));
    }
    Ok((bytes, req))
}

/// Downloads a file or a page, unless its site asked to be left alone.
async fn download(
    fetcher: &Fetcher,
    pace: &Pace,
    url: &str,
    from: Option<&str>,
    max_bytes: usize,
    watch: Watch<'_>,
) -> AppResult<Vec<u8>> {
    let host = Url::parse(url.trim())
        .map(|url| pace::site_key(&url))
        .unwrap_or_default();
    pace::wait_turn(pace, &host, TimeDelta::zero()).await?;
    match fetcher.get_from(url, from, max_bytes, watch).await {
        Err(FetchError::Status(status)) if pace::is_busy(status) => {
            pace.hold(&host, BUSY_WAIT_SECS, Utc::now());
            Err(AppError::TooManyRequests {
                retry_after: BUSY_WAIT_SECS,
            })
        }
        other => other.map_err(download_error),
    }
}

/// A failed download, in terms of what the user can change.
pub fn download_error(e: FetchError) -> AppError {
    match e {
        FetchError::BadUrl | FetchError::Blocked => AppError::BadRequest(e.to_string()),
        FetchError::TooLarge => AppError::PayloadTooLarge,
        _ => AppError::Upstream(e.to_string()),
    }
}

/// The address is a web page with nothing to import on it. When its site was
/// asked first and wanted an account, that is the likelier reason.
fn no_picture(refused: Option<Option<u16>>) -> AppError {
    match refused {
        Some(Some(401 | 403)) => AppError::BadRequest(
            "that site only answers with an account, and its page shows no picture without one"
                .into(),
        ),
        _ => AppError::Unsupported(
            "that address is a web page, and no picture or video was found on it".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use starbooru_core::Rating;

    use super::*;
    use crate::fetch::UNWATCHED;

    fn req(url: &str, source: Option<&str>) -> ImportRequest {
        ImportRequest {
            url: url.into(),
            tags: vec!["mine".into(), "sky".into()],
            rating: Rating::Explicit,
            source: source.map(str::to_owned),
        }
    }

    #[test]
    fn only_a_bare_address_is_looked_up() {
        assert!(wants_lookup(&req("https://b.org/posts/1", None)));
        assert!(wants_lookup(&req(
            "https://b.org/posts/1",
            Some(" https://b.org/posts/1 ")
        )));
        assert!(!wants_lookup(&req(
            "https://cdn.b.org/a.jpg",
            Some("https://b.org/posts/1")
        )));
    }

    /// A site shaped like a Gelbooru fork: its API answers for post 7 (unless
    /// `locked`), and its pages are there for 7 (a post) and 8 (no picture).
    async fn fake_booru(locked: bool) -> String {
        use axum::extract::{Query, State};
        use axum::http::StatusCode;
        use axum::response::{Html, IntoResponse, Response};
        use axum::routing::get;
        use std::collections::HashMap;

        async fn index(
            State((site, locked)): State<(String, bool)>,
            Query(q): Query<HashMap<String, String>>,
        ) -> Response {
            let is = |key: &str, value: &str| q.get(key).map(String::as_str) == Some(value);
            if is("page", "dapi") {
                if locked {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                if !is("id", "7") {
                    return axum::Json(serde_json::json!({"@attributes": {"count": 0}}))
                        .into_response();
                }
                return axum::Json(serde_json::json!({"post": [{"id": 7,
                    "file_url": format!("{site}/images/a.png"),
                    "preview_url": format!("{site}/thumbnails/a.jpg"),
                    "tags": "some_one sea -_-", "rating": "questionable",
                    "width": 16, "height": 12}]}))
                .into_response();
            }
            if is("id", "7") {
                return Html(format!(
                    r#"<!DOCTYPE html><html><head>
                    <meta property="og:image" content="{site}/images/sample.png"></head><body>
                    <li class="tag-type-artist"><a href="index.php?page=post&amp;s=list&amp;tags=some_one">some one</a></li>
                    <li class="tag-type-general"><a href="index.php?page=post&amp;s=list&amp;tags=sea">sea</a></li>
                    <li>Rating: Sensitive<br /></li>
                    <li><a href="{site}/images/a.png">Original image</a></li></body></html>"#
                ))
                .into_response();
            }
            Html("<!DOCTYPE html><html><body>Nothing here</body></html>").into_response()
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site = format!("http://{}", listener.local_addr().unwrap());
        let png = crate::testing::png(16, 12, 3);
        let app = axum::Router::new()
            .route("/index.php", get(index))
            .route("/images/a.png", get(move || async move { png.clone() }))
            .with_state((site.clone(), locked));
        tokio::spawn(async move { axum::serve(listener, app).await });
        site
    }

    #[tokio::test]
    async fn a_post_page_becomes_its_file_with_tags_and_rating() {
        let fetcher = Fetcher::new(crate::fetch::Reach::Any).unwrap();
        let pace = Pace::default();
        let site = fake_booru(false).await;
        let page = format!("{site}/index.php?page=post&s=view&id=7");
        let (bytes, got) = fetch(&fetcher, &pace, req(&page, None), 1 << 20, UNWATCHED)
            .await
            .unwrap();
        assert_eq!(bytes, crate::testing::png(16, 12, 3));
        assert_eq!(got.url, format!("{site}/images/a.png"));
        // The API doesn't say which tag is the artist.
        assert_eq!(got.tags, ["mine", "sky", "some_one", "sea"]);
        assert_eq!(got.rating, Rating::Questionable);
        assert_eq!(got.source.as_deref(), Some(page.as_str()));
    }

    #[tokio::test]
    async fn a_page_is_read_when_the_api_wants_an_account() {
        // Regression: a link to a Gelbooru post failed with "text/html files
        // are not supported".
        let fetcher = Fetcher::new(crate::fetch::Reach::Any).unwrap();
        let pace = Pace::default();
        let site = fake_booru(true).await;
        let page = format!("{site}/index.php?page=post&s=view&id=7");
        let (bytes, got) = fetch(&fetcher, &pace, req(&page, Some(&page)), 1 << 20, UNWATCHED)
            .await
            .unwrap();
        assert_eq!(bytes, crate::testing::png(16, 12, 3));
        assert_eq!(got.url, format!("{site}/images/a.png"));
        assert_eq!(got.tags, ["mine", "sky", "artist:some_one", "sea"]);
        assert_eq!(got.rating, Rating::Sensitive);
        assert_eq!(got.source.as_deref(), Some(page.as_str()));

        // A page with no picture says so, and says why when the site refused.
        let empty = format!("{site}/index.php?page=post&s=view&id=8");
        let error = fetch(&fetcher, &pace, req(&empty, None), 1 << 20, UNWATCHED)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, AppError::BadRequest(m) if m.contains("account")),
            "{error}"
        );
        let open = fake_booru(false).await;
        let empty = format!("{open}/index.php?page=post&s=view&id=8");
        let error = fetch(&fetcher, &pace, req(&empty, None), 1 << 20, UNWATCHED)
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::Unsupported(_)), "{error}");
    }

    #[tokio::test]
    async fn a_file_or_a_search_result_is_downloaded_as_it_is() {
        let fetcher = Fetcher::new(crate::fetch::Reach::Any).unwrap();
        let pace = Pace::default();
        let site = fake_booru(false).await;
        let file = format!("{site}/images/a.png");
        let (_, got) = fetch(&fetcher, &pace, req(&file, None), 1 << 20, UNWATCHED)
            .await
            .unwrap();
        assert_eq!(
            (got.url.as_str(), got.rating),
            (file.as_str(), Rating::Explicit)
        );
        assert_eq!(got.tags, ["mine", "sky"]);
        assert_eq!(got.source, None);
        // A result from a search already has its post: nothing is asked again.
        let locked = fake_booru(true).await;
        let file = format!("{locked}/images/a.png");
        let page = format!("{locked}/index.php?page=post&s=view&id=7");
        let (_, got) = fetch(&fetcher, &pace, req(&file, Some(&page)), 1 << 20, UNWATCHED)
            .await
            .unwrap();
        assert_eq!(got.source.as_deref(), Some(page.as_str()));
        assert_eq!(got.rating, Rating::Explicit);
        let error = fetch(&fetcher, &pace, req(&file, None), 8, UNWATCHED)
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::PayloadTooLarge), "{error}");
    }

    #[tokio::test]
    async fn a_site_that_wants_an_account_is_asked_once() {
        use axum::http::StatusCode;
        use axum::routing::get;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let asked = Arc::new(AtomicUsize::new(0));
        let count = asked.clone();
        let app = axum::Router::new().route(
            "/index.php",
            get(move || {
                count.fetch_add(1, Ordering::SeqCst);
                async { StatusCode::UNAUTHORIZED }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let fetcher = Fetcher::new(crate::fetch::Reach::Any).unwrap();
        let pace = Pace::default();
        let page = format!("{site}/index.php?page=post&s=view&id=7");
        for _ in 0..3 {
            let found = find(&pace, &fetcher, &page).await.unwrap();
            assert!(matches!(found, Lookup::Refused(Some(401))));
        }
        assert_eq!(asked.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_busy_site_makes_the_import_wait() {
        use axum::http::StatusCode;
        use axum::routing::get;
        let app = axum::Router::new()
            .route("/a.png", get(|| async { StatusCode::TOO_MANY_REQUESTS }))
            .route("/b.png", get(|| async { crate::testing::png(4, 4, 1) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let fetcher = Fetcher::new(crate::fetch::Reach::Any).unwrap();
        let pace = Pace::default();
        let busy =
            |e: AppError| matches!(e, AppError::TooManyRequests { retry_after } if retry_after > 0);
        let error = fetch(
            &fetcher,
            &pace,
            req(&format!("{site}/a.png"), None),
            1 << 20,
            UNWATCHED,
        )
        .await
        .unwrap_err();
        assert!(busy(error));
        // The next file on that site waits too, without being asked for.
        let error = fetch(
            &fetcher,
            &pace,
            req(&format!("{site}/b.png"), None),
            1 << 20,
            UNWATCHED,
        )
        .await
        .unwrap_err();
        assert!(busy(error));
        // Whoever hasn't heard that isn't held back.
        let other = Pace::default();
        assert!(
            fetch(
                &fetcher,
                &other,
                req(&format!("{site}/b.png"), None),
                1 << 20,
                UNWATCHED
            )
            .await
            .is_ok()
        );
    }

    #[test]
    fn the_sites_tags_follow_the_users() {
        let tags = merged(
            &["mine".to_owned(), "sky".to_owned()],
            vec!["artist:x".to_owned(), "sky".to_owned(), "sea".to_owned()],
        );
        assert_eq!(tags, ["mine", "sky", "artist:x", "sea"]);
        let full: Vec<String> = (0..MAX_TAGS_PER_POST).map(|i| format!("t{i}")).collect();
        assert_eq!(
            merged(&full, vec!["extra".to_owned()]).len(),
            MAX_TAGS_PER_POST
        );
    }
}
