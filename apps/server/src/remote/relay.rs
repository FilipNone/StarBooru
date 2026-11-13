//! Pictures from sites that only hand their files to their own pages
//! (`starbooru_core::guarded_by`). A page of ours can't show those, so the
//! server fetches them, naming the site, behind an address it signed itself:
//! only what it put in a search answer can be asked for, so this is no open
//! door to the web. Like `/media`, the address stands in for a sign-in, as an
//! `<img>` can't send one.

use axum::extract::{Query, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use chrono::{TimeDelta, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use starbooru_core::{RemotePost, guarded_by};
use url::Url;

use super::pace;
use crate::error::{AppError, AppResult};
use crate::fetch::UNWATCHED;
use crate::state::AppState;

/// Addresses are good until the end of the next six-hour stretch, so the
/// same one is handed out, and cached, for a while.
const BUCKET_SECS: i64 = 6 * 3600;
/// Largest picture or video passed on.
const MAX_BYTES: usize = 48 * 1024 * 1024;
/// The time between two fetches from one file server.
const INTERVAL: TimeDelta = TimeDelta::milliseconds(60);

pub struct Relay {
    key: Vec<u8>,
}

impl Relay {
    pub fn new(secret: &[u8]) -> Self {
        let mut key = b"starbooru-remote-media-v1:".to_vec();
        key.extend_from_slice(secret);
        Self { key }
    }

    fn mac(&self, target: &str, exp: i64) -> Option<Hmac<Sha256>> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).ok()?;
        mac.update(format!("{exp}:{target}").as_bytes());
        Some(mac)
    }

    /// The address on this server that passes `target` on.
    pub fn url(&self, target: &str, now: i64) -> String {
        let exp = (now.div_euclid(BUCKET_SECS) + 2).saturating_mul(BUCKET_SECS);
        let sig = self
            .mac(target, exp)
            .map(|mac| hex::encode(mac.finalize().into_bytes()))
            .unwrap_or_default();
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair("u", target);
        query.append_pair("exp", &exp.to_string());
        query.append_pair("sig", &sig);
        format!("/api/v1/remote/media?{}", query.finish())
    }

    pub fn verify(&self, target: &str, exp: i64, sig: &str, now: i64) -> bool {
        if exp < now || exp > now.saturating_add(3 * BUCKET_SECS) {
            return false;
        }
        let Ok(sig) = hex::decode(sig) else {
            return false;
        };
        self.mac(target, exp)
            .is_some_and(|mac| mac.verify_slice(&sig).is_ok())
    }

    /// Points what a page shows of `post` at this server, where its site
    /// would refuse the page itself. A post with no sample shows its file.
    pub fn guard(&self, post: &mut RemotePost, now: i64) {
        if guarded_by(&post.preview_url).is_some() {
            post.preview_url = self.url(&post.preview_url, now);
        }
        let shown = post.sample_url.as_deref().unwrap_or(&post.file_url);
        if guarded_by(shown).is_some() {
            post.sample_url = Some(self.url(shown, now));
        }
    }
}

#[derive(Deserialize)]
pub struct Signed {
    u: String,
    exp: i64,
    sig: String,
}

/// `GET /remote/media`: a picture or video from another site, by an address
/// this server signed.
pub async fn serve(State(state): State<AppState>, Query(q): Query<Signed>) -> AppResult<Response> {
    let now = Utc::now();
    if !state.relay.verify(&q.u, q.exp, &q.sig, now.timestamp()) {
        return Err(AppError::Forbidden);
    }
    let fetcher = state.fetcher.as_ref().ok_or(AppError::NotFound)?;
    let host = Url::parse(&q.u)
        .map(|url| pace::site_key(&url))
        .map_err(|_| AppError::NotFound)?;
    pace::wait_turn(&state.pace, &host, INTERVAL).await?;
    let bytes = fetcher
        .get_from(&q.u, guarded_by(&q.u), MAX_BYTES, UNWATCHED)
        .await
        .map_err(|e| match e {
            crate::fetch::FetchError::Status(status) if pace::is_busy(status) => {
                state.pace.hold(&host, pace::BUSY_WAIT_SECS, Utc::now());
                AppError::TooManyRequests {
                    retry_after: pace::BUSY_WAIT_SECS,
                }
            }
            other => super::lookup::download_error(other),
        })?;
    // Only pictures and videos go on to the page, whatever the site sent.
    let kind = starbooru_core::detect_mime(&bytes)
        .filter(|mime| mime.starts_with("image/") || mime.starts_with("video/"))
        .ok_or_else(|| AppError::Unsupported("that isn't a picture or a video".into()))?;
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    if let Ok(kind) = HeaderValue::from_str(kind) {
        headers.insert(header::CONTENT_TYPE, kind);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=21600"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THUMB: &str = "https://img4.gelbooru.com/thumbnails/65/20/thumbnail_x.jpg?a=1&b=2";

    #[test]
    fn only_what_was_signed_is_passed_on() {
        let relay = Relay::new(b"test-secret-test-secret-test-secret!");
        let now = 1_800_000_000;
        let address = relay.url(THUMB, now);
        let query: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(address.split_once('?').unwrap().1.as_bytes())
                .into_owned()
                .collect();
        assert_eq!(query["u"], THUMB);
        let exp: i64 = query["exp"].parse().unwrap();
        let sig = &query["sig"];
        assert!(relay.verify(THUMB, exp, sig, now));
        // Good for hours, the same address meanwhile, and not for ever.
        assert_eq!(relay.url(THUMB, now + 60), address);
        assert!(relay.verify(THUMB, exp, sig, now + 6 * 3600));
        assert!(!relay.verify(THUMB, exp, sig, exp + 1));
        // Another file, a longer life or another server's signature: no.
        assert!(!relay.verify("https://img4.gelbooru.com/other.jpg", exp, sig, now));
        assert!(!relay.verify("http://127.0.0.1/secret", exp, sig, now));
        assert!(!relay.verify(THUMB, exp + BUCKET_SECS, sig, now));
        assert!(!relay.verify(THUMB, exp, "zz", now));
        let other = Relay::new(b"another-secret-another-secret-another");
        assert!(!other.verify(THUMB, exp, sig, now));
    }

    fn post(preview: &str, sample: Option<&str>, file: &str) -> RemotePost {
        serde_json::from_value(serde_json::json!({
            "id": 1, "page_url": "https://x/1", "preview_url": preview, "sample_url": sample,
            "file_url": file, "tags": [], "rating": "safe", "width": 1, "height": 1,
        }))
        .unwrap()
    }

    #[test]
    fn guards_only_the_sites_that_need_it() {
        let relay = Relay::new(b"test-secret-test-secret-test-secret!");
        let relayed =
            |url: &str| url.starts_with("/api/v1/remote/media?u=https%3A%2F%2Fimg4.gelbooru.com");
        let mut guarded = post(THUMB, None, "https://img4.gelbooru.com/images/a.png");
        relay.guard(&mut guarded, 0);
        assert!(relayed(&guarded.preview_url));
        // Nothing smaller to show: the page gets the file itself, relayed,
        // while an import still downloads it from the site.
        assert!(relayed(guarded.sample_url.as_deref().unwrap()));
        assert_eq!(guarded.file_url, "https://img4.gelbooru.com/images/a.png");
        let mut open = post(
            "https://cdn.donmai.us/360x360/a.jpg",
            None,
            "https://cdn.donmai.us/original/a.jpg",
        );
        let before = open.clone();
        relay.guard(&mut open, 0);
        assert_eq!(open, before);
    }
}
