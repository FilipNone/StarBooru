//! On-disk cache for thumbnails and the images the viewer shows
//! (desktop and Android).
//!
//! Signed media URLs change every few hours, so the webview's own cache
//! keeps missing. Media is content-addressed and never changes, so the app
//! keeps it by variant and file name (`thumb/<sha256>.jpg`) instead: the
//! page asks for `/sbcache/<variant>/<file>?<signature>`, the handler
//! answers from disk, or downloads `/media/<variant>/<file>?<signature>`
//! from the signed-in server, keeps it and answers. Least recently used
//! files go once the cache is over its limit (Settings › Storage).
//!
//! The web app has the browser's cache and uses the media URLs as they are.

use starbooru_client::Client;

/// Whether this build keeps a media cache (desktop and Android).
pub const AVAILABLE: bool = cfg!(any(feature = "desktop", feature = "mobile"));
/// The page's path prefix that the handler answers.
pub const PREFIX: &str = "sbcache";
/// Largest file kept; bigger originals load straight from the server.
pub const MAX_FILE_BYTES: u64 = 64 << 20;
/// Limits offered in Settings, in MiB. 0 turns the cache off.
pub const LIMITS_MB: [u32; 6] = [0, 256, 512, 1024, 2048, 5120];
pub const DEFAULT_LIMIT_MB: u32 = 512;

/// A limit from [`LIMITS_MB`] for people: "512 MB", "2 GB".
pub fn limit_label(mb: u32) -> String {
    if mb >= 1024 && mb.is_multiple_of(1024) {
        format!("{} GB", mb / 1024)
    } else {
        format!("{mb} MB")
    }
}

/// A cacheable media file: its variant directory and file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key<'a> {
    pub variant: &'a str,
    pub file: &'a str,
}

#[cfg_attr(not(any(feature = "desktop", feature = "mobile")), allow(dead_code))]
impl Key<'_> {
    pub fn mime(&self) -> &'static str {
        match self.file.rsplit_once('.').map(|(_, ext)| ext) {
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            Some("avif") => "image/avif",
            Some("bmp") => "image/bmp",
            Some("tiff") => "image/tiff",
            Some("jxl") => "image/jxl",
            _ => "application/octet-stream",
        }
    }
}

/// Splits `<variant>/<sha256>.<ext>` into a key. Anything else, including
/// anything that could leave the cache directory, is `None`.
pub fn parse_key(path: &str) -> Option<Key<'_>> {
    let (variant, file) = path.split_once('/')?;
    if !matches!(variant, "thumb" | "sample" | "original") {
        return None;
    }
    let (sha, ext) = file.split_once('.')?;
    let hex = sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit());
    let ext_ok = (1..=5).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric());
    (hex && ext_ok).then_some(Key { variant, file })
}

/// `/media/<variant>/<file>?<query>` as the API returns it, split into the
/// key and the query (the signature).
pub fn split_media(path: &str) -> Option<(Key<'_>, &str)> {
    let rest = path.strip_prefix("/media/")?;
    let (key, query) = rest.split_once('?').unwrap_or((rest, ""));
    Some((parse_key(key)?, query))
}

/// The URL to show a media path with: through the cache on desktop and
/// Android when it is on, else straight from the server.
pub fn url(client: &Client, path: &str) -> String {
    match cached_path(path) {
        Some(p) if native::enabled() => p,
        _ => client.absolute(path),
    }
}

fn cached_path(path: &str) -> Option<String> {
    let (key, query) = split_media(path)?;
    Some(format!("/{PREFIX}/{}/{}?{query}", key.variant, key.file))
}

/// One file in the cache, for [`to_evict`].
#[cfg_attr(not(any(feature = "desktop", feature = "mobile")), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<P> {
    pub path: P,
    pub bytes: u64,
    /// Last use, as any ordered stamp (seconds since the epoch on disk).
    pub used: i64,
}

/// The least recently used files to delete so the rest fit in `limit`
/// bytes. Trims to nine tenths of the limit so it doesn't run on every
/// download.
#[cfg_attr(not(any(feature = "desktop", feature = "mobile")), allow(dead_code))]
pub fn to_evict<P>(mut entries: Vec<Entry<P>>, limit: u64) -> Vec<P> {
    let mut total: u64 = entries.iter().map(|e| e.bytes).sum();
    if total <= limit {
        return Vec::new();
    }
    let target = limit / 10 * 9;
    entries.sort_by_key(|e| e.used);
    let mut out = Vec::new();
    for e in entries {
        if total <= target {
            break;
        }
        total = total.saturating_sub(e.bytes);
        out.push(e.path);
    }
    out
}

pub use native::{clear, register, size};

#[cfg(any(feature = "desktop", feature = "mobile"))]
mod native {
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime};

    #[cfg(feature = "desktop")]
    use dioxus::desktop::{
        RequestAsyncResponder, use_asset_handler,
        wry::http::{HeaderValue, Response, StatusCode, header},
    };
    #[cfg(all(feature = "mobile", not(feature = "desktop")))]
    use dioxus::mobile::{
        RequestAsyncResponder, use_asset_handler,
        wry::http::{HeaderValue, Response, StatusCode, header},
    };
    use dioxus::prelude::*;

    use starbooru_client::Client;

    use super::{Entry, Key, MAX_FILE_BYTES, PREFIX, parse_key, to_evict};
    use crate::store::{self, PREFS};

    /// Bytes written since the last trim, so trimming runs now and then.
    static WRITTEN: AtomicU64 = AtomicU64::new(0);
    /// Names temporary files apart when two downloads overlap.
    static TMP: AtomicU64 = AtomicU64::new(0);

    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

    fn dir() -> Option<&'static Path> {
        DIR.get_or_init(|| {
            #[cfg(target_os = "android")]
            let dir = crate::system::android::cache_dir();
            #[cfg(not(target_os = "android"))]
            let dir = directories::ProjectDirs::from("eu", "filipnone", "StarBooru")
                .map(|d| d.cache_dir().to_path_buf());
            dir.map(|d| d.join("media"))
        })
        .as_deref()
    }

    fn limit() -> u64 {
        u64::from(PREFS.peek().cache_mb) << 20
    }

    pub fn enabled() -> bool {
        dir().is_some() && limit() > 0
    }

    /// Answers `/sbcache/…` for the page. Call once, from the root component.
    pub fn register() {
        #[cfg(target_os = "android")]
        let runtime = use_hook(|| tokio::runtime::Handle::try_current().ok());
        use_asset_handler(PREFIX, move |request, responder| {
            let path = request.uri().path().to_owned();
            let query = request.uri().query().unwrap_or_default().to_owned();
            // The page's state, which a task off the page can't reach.
            let (client, limit) = (store::client(), limit());
            let work = respond(path, query, client, limit, responder);
            // The page's own tasks only run once it has caught up, which it can't
            // while it waits on a slow answer. So on Android, where the WebView
            // waits for answers on threads of its own, they don't depend on it.
            // Elsewhere the answer has to come from the thread of the webview.
            #[cfg(target_os = "android")]
            if let Some(runtime) = &runtime {
                runtime.spawn(work);
                return;
            }
            spawn(work);
        });
        // Trim once at start, in case the limit was lowered.
        use_hook(|| {
            let limit = limit();
            spawn(async move {
                forget_old_thumbs().await;
                trim(limit).await;
            });
        });
    }

    async fn respond(
        path: String,
        query: String,
        client: Option<Client>,
        limit: u64,
        responder: RequestAsyncResponder,
    ) {
        responder.respond(answer(&path, &query, client, limit).await);
    }

    /// How long a download can take before the page is told it failed. Each one
    /// holds up a thread the WebView waits on, and there are many thumbnails on a
    /// screen, so one that isn't coming shouldn't keep it for long. Still well
    /// over what a slow connection needs, as a thumbnail given up on stays blank.
    fn patience(variant: &str) -> Duration {
        Duration::from_secs(match variant {
            "thumb" => 60,
            "sample" => 120,
            _ => 240,
        })
    }

    pub(super) async fn answer(
        path: &str,
        query: &str,
        client: Option<Client>,
        limit: u64,
    ) -> Response<Vec<u8>> {
        let key = path
            .trim_start_matches('/')
            .strip_prefix(PREFIX)
            .and_then(|p| parse_key(p.trim_start_matches('/')));
        let (Some(key), Some(root)) = (key, dir()) else {
            return status(StatusCode::NOT_FOUND);
        };
        let file = root.join(key.variant).join(key.file);
        if let Ok(bytes) = tokio::fs::read(&file).await {
            touch(file);
            return found(&key, bytes);
        }
        let Some(client) = client else {
            return status(StatusCode::UNAUTHORIZED);
        };
        let media = format!("/media/{}/{}?{query}", key.variant, key.file);
        let download = client.media_bytes(&media, MAX_FILE_BYTES);
        match tokio::time::timeout(patience(key.variant), download).await {
            Ok(Ok(bytes)) => {
                if limit > 0 {
                    keep(file, bytes.clone(), limit);
                }
                found(&key, bytes)
            }
            Ok(Err(_)) => status(StatusCode::BAD_GATEWAY),
            Err(_) => status(StatusCode::GATEWAY_TIMEOUT),
        }
    }

    fn found(key: &Key<'_>, bytes: Vec<u8>) -> Response<Vec<u8>> {
        let mut resp = Response::new(bytes);
        let headers = resp.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(key.mime()));
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, max-age=21600, immutable"),
        );
        resp
    }

    fn status(code: StatusCode) -> Response<Vec<u8>> {
        let mut resp = Response::new(Vec::new());
        *resp.status_mut() = code;
        resp
    }

    /// Marks a file as just used, for the least-recently-used order.
    fn touch(file: PathBuf) {
        tokio::task::spawn_blocking(move || {
            let now: SystemTime = chrono::Utc::now().into();
            let _ = std::fs::File::options()
                .write(true)
                .open(file)
                .and_then(|f| f.set_modified(now));
        });
    }

    /// Writes a downloaded file (whole, through a temporary name) and trims
    /// once enough has been written since the last trim.
    fn keep(file: PathBuf, bytes: Vec<u8>, limit: u64) {
        let len = bytes.len() as u64;
        tokio::spawn(async move {
            let n = TMP.fetch_add(1, Ordering::Relaxed);
            let tmp = file.with_extension(format!("{n}.part"));
            let written = match file.parent() {
                Some(d) => tokio::fs::create_dir_all(d).await,
                None => Ok(()),
            };
            let written = match written {
                Ok(()) => tokio::fs::write(&tmp, &bytes).await,
                Err(e) => Err(e),
            };
            let written = match written {
                Ok(()) => tokio::fs::rename(&tmp, &file).await,
                Err(e) => Err(e),
            };
            if written.is_err() {
                // A full disk or a missing directory: the page still got
                // the file, only the copy is lost.
                let _ = tokio::fs::remove_file(&tmp).await;
                return;
            }
            let since = WRITTEN
                .fetch_add(len, Ordering::Relaxed)
                .saturating_add(len);
            if since > limit / 20 {
                WRITTEN.store(0, Ordering::Relaxed);
                trim(limit).await;
            }
        });
    }

    fn entries(root: &Path) -> Vec<Entry<PathBuf>> {
        let mut out = Vec::new();
        let Ok(variants) = std::fs::read_dir(root) else {
            return out;
        };
        for variant in variants.flatten() {
            let Ok(files) = std::fs::read_dir(variant.path()) else {
                continue;
            };
            for f in files.flatten() {
                let Ok(meta) = f.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let used = meta
                    .modified()
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp())
                    .unwrap_or(0);
                out.push(Entry {
                    path: f.path(),
                    bytes: meta.len(),
                    used,
                });
            }
        }
        out
    }

    /// Deletes the least recently used files until the cache fits its limit.
    /// Files never go stale by their content, but the server can make a better
    /// thumbnail of the same picture. Raising this lets go of the ones kept so far.
    const THUMBS: &str = "2";

    /// Empties the thumbnails if they are from before `THUMBS`.
    pub(super) fn forget_thumbs_before(root: &Path, current: &str) {
        let mark = root.join("thumbs");
        if std::fs::read_to_string(&mark).is_ok_and(|seen| seen == current) {
            return;
        }
        let _ = std::fs::remove_dir_all(root.join("thumb"));
        if std::fs::create_dir_all(root).is_ok() {
            let _ = std::fs::write(mark, current);
        }
    }

    async fn forget_old_thumbs() {
        let Some(root) = dir() else { return };
        let _ = tokio::task::spawn_blocking(move || forget_thumbs_before(root, THUMBS)).await;
    }

    async fn trim(limit: u64) {
        let Some(root) = dir() else {
            return;
        };
        let _ = tokio::task::spawn_blocking(move || {
            for path in to_evict(entries(root), limit) {
                let _ = std::fs::remove_file(path);
            }
        })
        .await;
    }

    /// Bytes the cache takes on disk.
    pub async fn size() -> u64 {
        let Some(root) = dir() else { return 0 };
        tokio::task::spawn_blocking(move || entries(root).iter().map(|e| e.bytes).sum())
            .await
            .unwrap_or(0)
    }

    /// Deletes every cached file.
    pub async fn clear() {
        let Some(root) = dir() else { return };
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}

/// The web app leaves caching to the browser.
#[cfg(not(any(feature = "desktop", feature = "mobile")))]
mod native {
    pub fn enabled() -> bool {
        false
    }

    pub fn register() {}

    pub async fn size() -> u64 {
        0
    }

    pub async fn clear() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "desktop", feature = "mobile"))]
    #[test]
    fn old_thumbnails_go_once_and_the_rest_stay() {
        let root = std::env::temp_dir().join(format!("sb-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let file = |variant: &str| root.join(variant).join("file.jpg");
        let put = |variant: &str| {
            std::fs::create_dir_all(root.join(variant)).unwrap();
            std::fs::write(file(variant), b"x").unwrap();
        };
        put("thumb");
        put("sample");
        native::forget_thumbs_before(&root, "2");
        assert!(!file("thumb").exists());
        assert!(file("sample").exists());
        // Kept since then: they're fine, until the next change.
        put("thumb");
        native::forget_thumbs_before(&root, "2");
        assert!(file("thumb").exists());
        native::forget_thumbs_before(&root, "3");
        assert!(!file("thumb").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    // On Android answers run off the page, so they must need nothing of it: no
    // Dioxus runtime is running here, and reading the prefs without one panics.
    #[cfg(any(feature = "desktop", feature = "mobile"))]
    #[tokio::test]
    async fn an_answer_needs_nothing_of_the_page() {
        let path = format!("/{PREFIX}/thumb/{SHA}.jpg");
        let answer = native::answer(&path, "", None, 1 << 20).await;
        assert_eq!(answer.status(), 401);
        let answer = native::answer("/sbcache/nothing", "", None, 0).await;
        assert_eq!(answer.status(), 404);
    }

    const SHA: &str = "e04fd6110d750482f0a5cdbcba6ee9113004e1abc9a976a45cc9c1df5b9d4baf";

    #[test]
    fn keys_are_strict() {
        let thumb = format!("thumb/{SHA}.jpg");
        assert_eq!(
            parse_key(&thumb),
            Some(Key {
                variant: "thumb",
                file: &thumb[6..]
            })
        );
        assert!(parse_key(&format!("original/{SHA}.jpeg")).is_some());
        for bad in [
            format!("other/{SHA}.jpg"),
            format!("thumb/{SHA}"),
            format!("thumb/{SHA}."),
            format!("thumb/{SHA}.toolong"),
            format!("thumb/{SHA}.j/g"),
            format!("thumb/../{SHA}.jpg"),
            format!("thumb/{}.jpg", &SHA[1..]),
            format!("thumb/{}x.jpg", &SHA[1..]),
            "thumb/..".to_owned(),
            String::new(),
        ] {
            assert_eq!(parse_key(&bad), None, "{bad}");
        }
    }

    #[test]
    fn splits_signed_media_paths() {
        let path = format!("/media/sample/{SHA}.jpg?exp=1&sig=ab");
        let (key, query) = split_media(&path).unwrap();
        assert_eq!(key.variant, "sample");
        assert_eq!(key.mime(), "image/jpeg");
        assert_eq!(query, "exp=1&sig=ab");
        assert_eq!(
            cached_path(&path).unwrap(),
            format!("/sbcache/sample/{SHA}.jpg?exp=1&sig=ab")
        );
        assert!(split_media(&format!("/api/sample/{SHA}.jpg")).is_none());
        assert!(split_media("/media/sample/x.jpg?sig=1").is_none());
    }

    #[test]
    fn labels_limits() {
        assert_eq!(limit_label(512), "512 MB");
        assert_eq!(limit_label(1024), "1 GB");
        assert_eq!(limit_label(5120), "5 GB");
        assert_eq!(limit_label(1536), "1536 MB");
    }

    #[test]
    fn evicts_the_least_recently_used() {
        let e = |path, bytes, used| Entry { path, bytes, used };
        let files = vec![e("new", 40, 30), e("old", 40, 10), e("mid", 40, 20)];
        assert!(to_evict(files.clone(), 120).is_empty());
        // Over 100 bytes: trim to 90, which drops the oldest.
        assert_eq!(to_evict(files.clone(), 100), ["old"]);
        // Over 60: trim to 54, which takes two.
        assert_eq!(to_evict(files, 60), ["old", "mid"]);
        assert_eq!(to_evict(vec![e("a", 5, 1)], 0), ["a"]);
    }
}
