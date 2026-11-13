//! The compiled web app (`STARBOORU_WEB_DIR`): its files, how long browsers
//! keep them, and the page that starts it.
//!
//! `dx bundle` names every asset after its content (`main-dxh1a2b….css`), so
//! those never change and browsers keep them for a year without asking again.
//! `index.html` names the current ones, so it is checked on every visit.
//!
//! The page as `dx` writes it names its script and the stylesheet the app
//! links from the start. The browser then learns about the wasm from the
//! script, and about any other stylesheet from the running app, one round trip
//! after another. [`with_preloads`] names those in the page too, so everything
//! downloads together.
//!
//! `starbooru-server precompress <dir>` ([`precompress`]) writes a `.br` and a
//! `.gz` next to each text file. They are served as they are, which is smaller
//! than compressing on every request and costs the server nothing. Without
//! them the files are compressed as they go out.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use tower::ServiceExt;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;

/// For files named after their content.
const KEEP: &str = "public, max-age=31536000, immutable";
/// For everything else: kept, but checked with the server before each use.
const CHECK: &str = "no-cache";

/// Larger pages, scripts and binaries than these aren't read for preloads.
const MAX_PAGE_BYTES: u64 = 1 << 20;
const MAX_SCRIPT_BYTES: u64 = 8 << 20;
const MAX_WASM_BYTES: u64 = 128 << 20;

/// What `precompress` compresses, by extension. Images and fonts are
/// compressed already.
const TEXT: [&str; 9] = [
    "html", "js", "mjs", "css", "wasm", "svg", "json", "txt", "map",
];
/// Smaller files than this aren't worth a second copy.
const MIN_COMPRESS_BYTES: u64 = 1024;

struct WebApp {
    files: ServeDir,
    /// `index.html` with the preloads added.
    page: String,
}

/// The service that answers every path the API doesn't. `None` when the
/// directory has no `index.html`.
pub fn service(dir: &Path) -> Option<Router> {
    let page = read_capped(&dir.join("index.html"), MAX_PAGE_BYTES)
        .and_then(|bytes| String::from_utf8(bytes).ok())?;
    let app = Arc::new(WebApp {
        // The page comes from `page`, so a directory is just not found.
        files: ServeDir::new(dir)
            .precompressed_br()
            .precompressed_gzip()
            .append_index_html_on_directories(false),
        page: with_preloads(&page, dir),
    });
    Some(
        Router::new()
            .fallback(move |req: Request| {
                let app = app.clone();
                async move { app.serve(req).await }
            })
            .layer(CompressionLayer::new()),
    )
}

impl WebApp {
    async fn serve(&self, req: Request) -> Response {
        let path = req.uri().path().to_owned();
        let reads = matches!(*req.method(), Method::GET | Method::HEAD);
        if reads && matches!(path.as_str(), "/" | "/index.html") {
            return self.page();
        }
        let mut response = match self.files.clone().oneshot(req).await {
            Ok(found) => found.map(Body::new),
            Err(never) => match never {},
        };
        let status = response.status();
        // Single-page app: the router in the page knows the other paths. A
        // missing asset stays missing, as a page is no use to a script tag.
        if status == StatusCode::NOT_FOUND && reads && !path.starts_with("/assets/") {
            return self.page();
        }
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            let keep = HeaderValue::from_static(cache_policy(&path));
            response.headers_mut().insert(header::CACHE_CONTROL, keep);
        }
        response
    }

    fn page(&self) -> Response {
        let mut response = Response::new(Body::from(self.page.clone()));
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(CHECK));
        response
    }
}

/// How long a browser may keep the file at `path`.
fn cache_policy(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or_default();
    if path.starts_with("/assets/") && is_hashed(name) {
        KEEP
    } else {
        CHECK
    }
}

/// Whether `dx` named this file after its content: `<name>-dxh<hex>.<ext>`.
fn is_hashed(name: &str) -> bool {
    let Some((_, rest)) = name.rsplit_once("-dxh") else {
        return false;
    };
    let hash = rest.split('.').next().unwrap_or_default();
    hash.len() >= 8 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The file a URL path names inside `dir`, or `None` if it could leave it.
fn local(dir: &Path, url_path: &str) -> Option<PathBuf> {
    let mut out = dir.to_path_buf();
    for part in url_path.split('/') {
        match part {
            "" | "." => {}
            part if Path::new(part)
                .components()
                .all(|c| matches!(c, Component::Normal(_))) =>
            {
                out.push(part);
            }
            _ => return None,
        }
    }
    Some(out)
}

fn read_capped(path: &Path, max: u64) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    (meta.is_file() && meta.len() <= max)
        .then(|| std::fs::read(path).ok())
        .flatten()
}

/// Only what `dx` puts in an asset's address, so nothing needs escaping.
fn is_plain(url: &str) -> bool {
    !url.is_empty()
        && url
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_'))
}

/// The first `src="….js"` in the page: the script that loads the app.
fn script_of(page: &str) -> Option<&str> {
    page.split("src=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .find(|src| src.ends_with(".js") && is_plain(src))
}

/// The quoted address in the loader script that ends in `.wasm` and is named
/// after its content. The script also holds wasm-bindgen's default name
/// (`starbooru-app_bg.wasm`), which is no file of the bundle's.
fn wasm_of(script: &str) -> Option<&str> {
    let ext = ".wasm\"";
    let mut rest = script;
    while let Some(hit) = rest.find(ext) {
        let end = hit.checked_add(".wasm".len())?;
        let head = rest.get(..end)?;
        let url = head
            .rfind('"')
            .and_then(|quote| head.get(quote.checked_add(1)?..));
        if let Some(url) = url.filter(|url| is_plain(url))
            && is_hashed(url.rsplit('/').next().unwrap_or_default())
        {
            return Some(url);
        }
        rest = rest.get(hit.checked_add(ext.len())?..)?;
    }
    None
}

/// The stylesheets a wasm binary names (`main-dxh1a2b….css`), which is where
/// the app keeps the address of each asset it uses.
fn stylesheets_of(wasm: &[u8]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let ext = b".css";
    let mut at = 0usize;
    while let Some(hit) = wasm
        .get(at..)
        .and_then(|rest| rest.windows(ext.len()).position(|w| w == ext))
    {
        let end = at.saturating_add(hit).saturating_add(ext.len());
        let start = wasm.get(..end.saturating_sub(ext.len())).map_or(0, |head| {
            head.iter()
                .rposition(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
                .map_or(0, |i| i.saturating_add(1))
        });
        if let Some(name) = wasm
            .get(start..end)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .filter(|name| is_hashed(name))
        {
            found.insert(name.to_owned());
        }
        at = end;
    }
    found
}

/// `page` with a preload for the wasm and for each stylesheet the app uses
/// that the page doesn't link itself, found by following the page to its
/// script and the script to its wasm. Anything that can't be found is left
/// out, and the page works as before.
fn with_preloads(page: &str, dir: &Path) -> String {
    let mut links = String::new();
    let script = script_of(page)
        .and_then(|src| local(dir, src))
        .and_then(|path| read_capped(&path, MAX_SCRIPT_BYTES))
        .and_then(|bytes| String::from_utf8(bytes).ok());
    if let Some(wasm) = script.as_deref().and_then(wasm_of) {
        // `fetch` only takes a preload made the same way: as a fetch, CORS-anonymous.
        links.push_str(&format!(
            "<link rel=\"preload\" as=\"fetch\" type=\"application/wasm\" href=\"{wasm}\" crossorigin>"
        ));
        let binary = local(dir, wasm).and_then(|path| read_capped(&path, MAX_WASM_BYTES));
        for name in binary.as_deref().map(stylesheets_of).unwrap_or_default() {
            // A stylesheet in the page is on its way already.
            if !page.contains(&name) && dir.join("assets").join(&name).is_file() {
                links.push_str(&format!(
                    "<link rel=\"preload\" as=\"style\" href=\"/assets/{name}\">"
                ));
            }
        }
    }
    match page.split_once("</head>") {
        Some((head, rest)) if !links.is_empty() => format!("{head}{links}</head>{rest}"),
        _ => page.to_owned(),
    }
}

/// What `precompress` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Precompressed {
    pub files: usize,
    pub bytes: u64,
    pub brotli_bytes: u64,
}

/// Writes `<file>.br` and `<file>.gz` next to every text file under `dir`, at
/// the strongest settings: it runs once, at build time. A copy that isn't
/// smaller than its file is left out.
pub fn precompress(dir: &Path) -> std::io::Result<Precompressed> {
    let mut done = Precompressed::default();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        // Listed first and in order, so a run doesn't depend on how the file
        // system hands out names.
        let mut paths = std::fs::read_dir(&folder)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            let meta = match std::fs::metadata(&path) {
                Ok(meta) => meta,
                // A copy from an earlier run, removed since the folder was listed.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if meta.is_dir() {
                folders.push(path);
                continue;
            }
            let text = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| TEXT.contains(&e));
            if !text || meta.len() < MIN_COMPRESS_BYTES {
                continue;
            }
            let bytes = std::fs::read(&path)?;
            let mut brotli = Vec::new();
            let params = brotli::enc::BrotliEncoderParams {
                quality: 11,
                lgwin: 24,
                size_hint: bytes.len(),
                ..Default::default()
            };
            brotli::BrotliCompress(&mut bytes.as_slice(), &mut brotli, &params)?;
            let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            gzip.write_all(&bytes)?;
            let gzip = gzip.finish()?;
            done.files = done.files.saturating_add(1);
            done.bytes = done.bytes.saturating_add(bytes.len() as u64);
            done.brotli_bytes = done
                .brotli_bytes
                .saturating_add(brotli.len().min(bytes.len()) as u64);
            for (ext, copy) in [("br", brotli), ("gz", gzip)] {
                let mut name = path.clone().into_os_string();
                name.push(".");
                name.push(ext);
                let copy_path = PathBuf::from(name);
                if copy.len() < bytes.len() {
                    std::fs::write(&copy_path, copy)?;
                } else {
                    // One from an earlier run would be served for the new file.
                    let _ = std::fs::remove_file(&copy_path);
                }
            }
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use axum::body::to_bytes;
    use axum::http::Request;

    use super::*;

    const WASM: &str = "starbooru-app_bg-dxh9b1a36805432b663.wasm";
    const SCRIPT: &str = "starbooru-app-dxhfada5334a65d0b9.js";
    const STYLE: &str = "main-dxh1320afb2b19950b.css";

    /// A directory like the one `dx bundle` writes.
    fn bundle() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            format!(
                "<!DOCTYPE html><html><head><title>StarBooru</title>\
                 <link rel=\"preload\" as=\"script\" href=\"/./assets/{SCRIPT}\" crossorigin></head>\
                 <body><div id=\"main\"></div>\
                 <script type=\"module\" async src=\"/./assets/{SCRIPT}\"></script></body></html>"
            ),
        )
        .unwrap();
        std::fs::write(
            assets.join(SCRIPT),
            // As wasm-bindgen writes it: its default name first, then the real one.
            format!(
                "e=new URL(\"starbooru-app_bg.wasm\",import.meta.url);\
                 st({{module_or_path:\"/./assets/{WASM}\"}}).then(u=>{{}})"
            ),
        )
        .unwrap();
        let mut wasm = b"\0asm\x01\0\0\0lbundled_pathX\x1b".to_vec();
        wasm.extend_from_slice(STYLE.as_bytes());
        wasm.extend_from_slice(b"goptions\xa0 old-dxh00000000aaaa.css and plain.css");
        std::fs::write(assets.join(WASM), wasm).unwrap();
        std::fs::write(assets.join(STYLE), "body { margin: 0 } ".repeat(200)).unwrap();
        dir
    }

    async fn get(app: &Router, path: &str, accept: Option<&str>) -> (Response, Vec<u8>) {
        let mut req = Request::get(path);
        if let Some(accept) = accept {
            req = req.header(header::ACCEPT_ENCODING, accept);
        }
        let response = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (parts, body) = response.into_parts();
        let body = to_bytes(body, usize::MAX).await.unwrap().to_vec();
        (Response::from_parts(parts, Body::empty()), body)
    }

    fn cache(response: &Response) -> &str {
        response.headers()[header::CACHE_CONTROL].to_str().unwrap()
    }

    #[test]
    fn only_files_named_after_their_content_are_kept_for_good() {
        assert_eq!(cache_policy(&format!("/assets/{STYLE}")), KEEP);
        assert_eq!(cache_policy(&format!("/assets/{WASM}")), KEEP);
        assert_eq!(cache_policy("/assets/logo.svg"), CHECK);
        assert_eq!(cache_policy("/assets/main-dxh.css"), CHECK);
        assert_eq!(cache_policy("/assets/main-dxhnothexatall.css"), CHECK);
        assert_eq!(cache_policy("/index.html"), CHECK);
        // Outside `assets`, a name proves nothing.
        assert_eq!(cache_policy(&format!("/{STYLE}")), CHECK);
    }

    #[test]
    fn paths_stay_inside_the_directory() {
        let dir = Path::new("/srv/web");
        assert_eq!(
            local(dir, "/./assets/a.js"),
            Some(PathBuf::from("/srv/web/assets/a.js"))
        );
        assert_eq!(local(dir, "/assets/../../etc/passwd"), None);
        assert_eq!(local(dir, "/assets/..%2f"), Some(dir.join("assets/..%2f")));
    }

    #[test]
    fn the_page_leads_to_its_script_wasm_and_stylesheet() {
        let dir = bundle();
        let page = std::fs::read_to_string(dir.path().join("index.html")).unwrap();
        assert_eq!(script_of(&page), Some(&*format!("/./assets/{SCRIPT}")));
        let script = std::fs::read_to_string(dir.path().join("assets").join(SCRIPT)).unwrap();
        assert_eq!(wasm_of(&script), Some(&*format!("/./assets/{WASM}")));
        let wasm = std::fs::read(dir.path().join("assets").join(WASM)).unwrap();
        // The unhashed name isn't an asset of the bundle's.
        assert_eq!(
            stylesheets_of(&wasm),
            BTreeSet::from([STYLE.to_owned(), "old-dxh00000000aaaa.css".to_owned()])
        );
    }

    #[test]
    fn the_page_preloads_what_exists() {
        let dir = bundle();
        let page = std::fs::read_to_string(dir.path().join("index.html")).unwrap();
        let out = with_preloads(&page, dir.path());
        assert!(out.contains(&format!(
            "<link rel=\"preload\" as=\"fetch\" type=\"application/wasm\" href=\"/./assets/{WASM}\" crossorigin>\
             <link rel=\"preload\" as=\"style\" href=\"/assets/{STYLE}\"></head>"
        )));
        // Named in the wasm, but not a file here.
        assert!(!out.contains("old-dxh"));
        assert!(out.ends_with("</body></html>"));
    }

    #[test]
    fn a_stylesheet_the_page_links_is_not_preloaded_as_well() {
        let dir = bundle();
        let page = std::fs::read_to_string(dir.path().join("index.html"))
            .unwrap()
            .replace(
                "</body>",
                &format!("<link rel=\"stylesheet\" href=\"/./assets/{STYLE}\"></body>"),
            );
        let out = with_preloads(&page, dir.path());
        assert!(out.contains(&format!("href=\"/./assets/{WASM}\" crossorigin>")));
        assert!(!out.contains("as=\"style\""));
        assert_eq!(out.matches(STYLE).count(), 1);
    }

    #[test]
    fn a_page_it_cannot_follow_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        for page in [
            "<html><head></head><body>no script</body></html>",
            "<html><body><script src=\"/assets/gone.js\"></script></body></html>",
            "<script src=\"/assets/../../../etc/passwd.js\"></script></head>",
            "<script src=\"/a&quot;b.js\"></script></head>",
        ] {
            assert_eq!(with_preloads(page, dir.path()), page);
        }
        assert_eq!(wasm_of("no wasm here"), None);
        assert_eq!(wasm_of(".wasm\""), None);
        assert_eq!(wasm_of("\"a b.wasm\""), None);
        // Regression: the default name came first and was taken for the file.
        assert_eq!(wasm_of("u(\"app_bg.wasm\")"), None);
        assert_eq!(
            wasm_of("u(\"app_bg.wasm\");v(\"/assets/app_bg-dxh0123456789abcdef.wasm\")"),
            Some("/assets/app_bg-dxh0123456789abcdef.wasm")
        );
        assert!(stylesheets_of(b".css.css\xff-dxh.css").is_empty());
    }

    #[tokio::test]
    async fn every_page_of_the_app_is_the_prepared_page() {
        let dir = bundle();
        let app = service(dir.path()).unwrap();
        for path in ["/", "/index.html", "/post/12", "/settings", "/assets"] {
            let (response, body) = get(&app, path, None).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(cache(&response), CHECK, "{path}");
            assert!(
                response.headers()[header::CONTENT_TYPE]
                    .to_str()
                    .unwrap()
                    .starts_with("text/html")
            );
            let page = String::from_utf8(body).unwrap();
            assert!(page.contains("as=\"fetch\""), "{path}");
        }
    }

    #[tokio::test]
    async fn assets_are_kept_and_a_missing_one_is_not_a_page() {
        let dir = bundle();
        let app = service(dir.path()).unwrap();
        let (response, body) = get(&app, &format!("/assets/{STYLE}"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(cache(&response), KEEP);
        assert!(body.starts_with(b"body {"));
        // Regression: a stale page asking for last version's wasm got HTML back.
        let (response, _) = get(&app, "/assets/app_bg-dxh0123456789abcdef.wasm", None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn no_index_no_web_app() {
        let dir = tempfile::tempdir().unwrap();
        assert!(service(dir.path()).is_none());
    }

    #[tokio::test]
    async fn precompressed_copies_are_served_as_they_are() {
        let dir = bundle();
        let done = precompress(dir.path()).unwrap();
        // The stylesheet only: the other files are under a kilobyte.
        assert_eq!(done.files, 1);
        assert!(done.brotli_bytes < done.bytes / 4);
        let style = dir.path().join("assets").join(STYLE);
        let original = std::fs::read(&style).unwrap();
        let brotli = std::fs::read(style.with_extension("css.br")).unwrap();
        let mut unpacked = Vec::new();
        brotli::Decompressor::new(brotli.as_slice(), 4096)
            .read_to_end(&mut unpacked)
            .unwrap();
        assert_eq!(unpacked, original);
        let mut unpacked = Vec::new();
        flate2::read::GzDecoder::new(
            std::fs::read(style.with_extension("css.gz"))
                .unwrap()
                .as_slice(),
        )
        .read_to_end(&mut unpacked)
        .unwrap();
        assert_eq!(unpacked, original);

        let app = service(dir.path()).unwrap();
        let path = format!("/assets/{STYLE}");
        let (response, body) = get(&app, &path, Some("gzip, br")).await;
        assert_eq!(response.headers()[header::CONTENT_ENCODING], "br");
        assert_eq!(cache(&response), KEEP);
        assert_eq!(body, brotli);
        let (response, _) = get(&app, &path, Some("gzip")).await;
        assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
        let (response, body) = get(&app, &path, None).await;
        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        assert_eq!(body, original);
    }

    #[test]
    fn a_copy_that_saves_nothing_is_not_kept() {
        // Regression: the stale copy is listed after its file. Once the file's
        // turn had removed it, its own turn failed the whole run as not found.
        let dir = tempfile::tempdir().unwrap();
        // Random bytes don't compress.
        let mut noise = vec![0u8; 4096];
        rand::fill(noise.as_mut_slice());
        let file = dir.path().join("noise.wasm");
        std::fs::write(&file, &noise).unwrap();
        std::fs::write(file.with_extension("wasm.br"), b"from an earlier build").unwrap();
        std::fs::write(dir.path().join("photo.png"), vec![7u8; 4096]).unwrap();
        precompress(dir.path()).unwrap();
        assert!(!file.with_extension("wasm.br").exists());
        assert!(!file.with_extension("wasm.gz").exists());
        assert!(!dir.path().join("photo.png.br").exists());
    }
}
