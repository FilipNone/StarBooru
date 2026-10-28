//! Text in images (OCR), through the `tesseract` command-line tool.
//!
//! Local by design: tesseract runs as a child process on the stored file
//! (the sample when there is one, else the original), with a time limit and
//! capped output. No image leaves the server. Only still images are
//! scanned. The text found is kept in `posts.ocr_text` and searched with
//! `text:` terms. Never log it: it is part of the private collection.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::state::AppState;
use crate::storage::{Storage, Variant};

const TIMEOUT: Duration = Duration::from_secs(60);
/// Raw tesseract output read at most; the cleaned text is capped below.
const MAX_OUTPUT: u64 = 256 * 1024;
/// Longest text stored for a post, in characters.
pub const MAX_TEXT_CHARS: usize = 10_000;
/// At most this many tesseract processes at once, so a burst of uploads
/// can't swamp the server.
static RUNNING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
/// Originals tesseract (leptonica) reads directly.
const READABLE: &[&str] = &["jpg", "png", "webp", "bmp", "tiff"];

/// The tesseract program and the languages it reads (`eng`, `eng+ita`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ocr {
    pub program: PathBuf,
    pub langs: String,
}

impl Ocr {
    /// Whether the program runs at all. Logs once when it doesn't.
    pub fn available(&self) -> bool {
        let ok = Command::new(&self.program)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            warn!(
                "{} not found: text in images (OCR) is off",
                self.program.to_string_lossy()
            );
        }
        ok
    }

    /// Reads the text in the image at `path`, cleaned. `None` when
    /// tesseract fails or times out. Call from a blocking thread.
    pub fn scan(&self, path: &Path) -> Option<String> {
        let mut cmd = Command::new(&self.program);
        cmd.arg(path)
            .arg("stdout")
            .arg("-l")
            .arg(&self.langs)
            .env("OMP_THREAD_LIMIT", "1");
        let raw = run(&mut cmd, MAX_OUTPUT, TIMEOUT)?;
        Some(clean(&String::from_utf8_lossy(&raw)))
    }
}

/// Tidies OCR output: trims each line, collapses runs of whitespace, drops
/// lines with no letter or digit and lone-character noise, and caps the
/// length at `MAX_TEXT_CHARS`. Lines stay separate.
pub fn clean(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = 0usize;
    for line in raw.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let alnum = line.chars().filter(|c| c.is_alphanumeric()).count();
        if alnum == 0 || (alnum == 1 && line.chars().count() <= 2) {
            continue;
        }
        let sep = usize::from(!out.is_empty());
        let len = line.chars().count();
        if chars.saturating_add(sep).saturating_add(len) > MAX_TEXT_CHARS {
            let room = MAX_TEXT_CHARS.saturating_sub(chars.saturating_add(sep));
            if room > 0 {
                if sep == 1 {
                    out.push('\n');
                }
                out.extend(line.chars().take(room));
            }
            break;
        }
        if sep == 1 {
            out.push('\n');
        }
        out.push_str(&line);
        chars = chars.saturating_add(sep).saturating_add(len);
    }
    out
}

/// The file to scan for a post, or `None` when it is not a still image
/// tesseract can read.
fn source_path(
    storage: &Storage,
    sha_hex: &str,
    ext: &str,
    has_sample: Option<bool>,
) -> Option<PathBuf> {
    if has_sample == Some(true) {
        let sample = storage.path_for_file(
            Variant::Sample,
            &Storage::file_name(Variant::Sample, sha_hex, ""),
        );
        if let Some(p) = sample.filter(|p| p.exists()) {
            return Some(p);
        }
    }
    if READABLE.contains(&ext) {
        return storage.original_path(sha_hex, ext);
    }
    None
}

/// sha256, ext, kind, has_sample.
type ScanRow = (Vec<u8>, String, i16, Option<bool>);

/// Scans one post and stores the result. Errors are logged, never returned:
/// OCR must not fail anything else.
pub async fn scan_post(state: &AppState, id: i64) {
    let Some(ocr) = state.ocr.clone() else {
        return;
    };
    let row: Result<Option<ScanRow>, _> =
        sqlx::query_as("SELECT sha256, ext, kind, has_sample FROM posts WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await;
    let (sha, ext, kind, has_sample) = match row {
        Ok(Some(r)) => r,
        Ok(None) => return,
        Err(e) => {
            warn!("OCR: post {id}: {e}");
            return;
        }
    };
    let path = (kind == 0)
        .then(|| source_path(&state.storage, &hex::encode(sha), &ext, has_sample))
        .flatten();
    let text = match path {
        Some(path) => {
            let Ok(_permit) = RUNNING.acquire().await else {
                return;
            };
            match tokio::task::spawn_blocking(move || ocr.scan(&path)).await {
                Ok(Some(text)) => Some(text),
                // Failed or timed out: marked as scanned, so the backfill
                // doesn't retry it at every start. A rescan can.
                Ok(None) => None,
                Err(e) => {
                    warn!("OCR: post {id}: {e}");
                    return;
                }
            }
        }
        None => None,
    };
    let text = text.filter(|t| !t.is_empty());
    let updated = sqlx::query("UPDATE posts SET ocr_text = $2, ocr_at = now() WHERE id = $1")
        .bind(id)
        .bind(text)
        .execute(&state.db)
        .await;
    if let Err(e) = updated {
        warn!("OCR: post {id}: {e}");
    }
}

/// Scans a new post in the background.
pub fn spawn_scan(state: &AppState, id: i64) {
    if state.ocr.is_none() {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move { scan_post(&state, id).await });
}

/// At startup: scans still images not scanned yet, one at a time.
pub async fn backfill(state: AppState) {
    if state.ocr.is_none() {
        return;
    }
    let mut after = 0i64;
    let mut done = 0u64;
    loop {
        let batch: Result<Vec<i64>, _> = sqlx::query_scalar(
            "SELECT id FROM posts WHERE ocr_at IS NULL AND kind = 0 AND id > $1 \
             ORDER BY id LIMIT 50",
        )
        .bind(after)
        .fetch_all(&state.db)
        .await;
        let batch = match batch {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => break,
            Err(e) => {
                warn!("OCR backfill stopped: {e}");
                return;
            }
        };
        for id in batch {
            after = id;
            scan_post(&state, id).await;
            done += 1;
        }
    }
    if done > 0 {
        info!("OCR backfill: scanned {done} posts");
    }
}

/// Runs a tool and returns its stdout if it exits successfully within
/// `timeout`. Output beyond `max_out` bytes stalls the child until the
/// timeout kills it.
fn run(cmd: &mut Command, max_out: u64, timeout: Duration) -> Option<Vec<u8>> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(max_out).read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + timeout;
    let ok = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let out = reader.join().ok()?;
    ok.then_some(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// tesseract with English on `PATH`, or `None` (and the test is skipped).
    pub fn ocr() -> Option<Ocr> {
        let ocr = Ocr {
            program: "tesseract".into(),
            langs: "eng".into(),
        };
        if ocr.available() {
            Some(ocr)
        } else {
            eprintln!("skipped: tesseract is not installed");
            None
        }
    }

    #[test]
    fn cleans_whitespace_and_noise() {
        let raw = "  Hello    World \n\n | \n — \n x\n\tSecond\tline  \n\u{c}";
        assert_eq!(clean(raw), "Hello World\nSecond line");
        assert_eq!(clean(""), "");
        assert_eq!(clean(" \n \n"), "");
        assert_eq!(clean("a1"), "a1");
    }

    /// A PNG with `text` in big black letters, drawn by ImageMagick, or
    /// `None` (and the test is skipped) without it.
    fn text_png(text: &str) -> Option<Vec<u8>> {
        let out = Command::new("magick")
            .args(["-background", "white", "-fill", "black", "-pointsize", "72"])
            .arg(format!("label:{text}"))
            .arg("png:-")
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success() && !o.stdout.is_empty());
        if out.is_none() {
            eprintln!("skipped: ImageMagick (magick) is not installed");
        }
        out.map(|o| o.stdout)
    }

    #[tokio::test]
    async fn text_search() {
        let Some(app) = crate::testing::app().await else {
            return;
        };
        let t = app.register("reader").await;
        let sign = app.upload_png(&t, 1, "sign").await;
        let menu = app.upload_png(&t, 2, "menu").await;
        let blank = app.upload_png(&t, 3, "blank").await;
        for (id, text) in [(sign, "Hello World\nNo parking"), (menu, "Pizza and pasta")] {
            sqlx::query("UPDATE posts SET ocr_text = $2, ocr_at = now() WHERE id = $1")
                .bind(id)
                .bind(text)
                .execute(app.db())
                .await
                .unwrap();
        }
        let ids = |q: &'static str| {
            let app = &app;
            let t = t.clone();
            async move {
                let r = app.get(&format!("/api/v1/posts?q={q}"), &t).await;
                assert_eq!(r.status, axum::http::StatusCode::OK, "{q}: {:?}", r.json());
                r.json()["posts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["id"].as_i64().unwrap())
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(ids("text:hello").await, vec![sign]);
        assert_eq!(ids("text:PIZZA").await, vec![menu]);
        assert_eq!(ids("text:%22no%20parking%22").await, vec![sign]);
        assert_eq!(ids("text:hello%20text:pizza").await, Vec::<i64>::new());
        assert_eq!(ids("-text:hello").await, vec![blank, menu]);
        assert_eq!(ids("menu%20text:pasta").await, vec![menu]);
        let r = app.get("/api/v1/posts?q=text:", &t).await;
        assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST);

        // The single post carries the text; lists and empty posts don't.
        let p = app.get(&format!("/api/v1/posts/{sign}"), &t).await.json();
        assert_eq!(p["ocr_text"], "Hello World\nNo parking");
        let p = app.get(&format!("/api/v1/posts/{blank}"), &t).await.json();
        assert!(p.get("ocr_text").is_none());
        let page = app.get("/api/v1/posts", &t).await.json();
        assert!(page["posts"][0].get("ocr_text").is_none());

        // OCR is off in tests: a rescan says so.
        let r = app
            .call(
                axum::http::Method::POST,
                &format!("/api/v1/posts/{sign}/ocr"),
                Some(&t),
                None,
            )
            .await;
        assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST);
        app.finish().await;
    }

    #[tokio::test]
    async fn reads_text_after_upload() {
        let (Some(ocr), Some(png)) = (ocr(), text_png("HELLO STARBOORU")) else {
            return;
        };
        let Some(app) = crate::testing::app_with(|c| c.ocr = Some(ocr)).await else {
            return;
        };
        let t = app.register("scanner").await;
        let r = app.upload(&t, &png, &[("tags", "sign")]).await;
        assert_eq!(r.status, axum::http::StatusCode::OK);
        let id = r.json()["id"].as_i64().unwrap();
        // The scan runs in the background; wait for it.
        let mut scanned = false;
        for _ in 0..300 {
            let at: Option<chrono::DateTime<chrono::Utc>> =
                sqlx::query_scalar("SELECT ocr_at FROM posts WHERE id = $1")
                    .bind(id)
                    .fetch_one(app.db())
                    .await
                    .unwrap();
            if at.is_some() {
                scanned = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(scanned, "OCR did not finish");
        let p = app.get(&format!("/api/v1/posts/{id}"), &t).await.json();
        let text = p["ocr_text"].as_str().unwrap().to_ascii_lowercase();
        assert!(text.contains("starbooru"), "{text}");
        let found = app.get("/api/v1/posts?q=text:starbooru", &t).await.json();
        assert_eq!(found["posts"][0]["id"].as_i64(), Some(id));

        // Not scanned yet: the backfill does it; a rescan works too.
        sqlx::query("UPDATE posts SET ocr_text = NULL, ocr_at = NULL")
            .execute(app.db())
            .await
            .unwrap();
        backfill(app.state.clone()).await;
        let p = app.get(&format!("/api/v1/posts/{id}"), &t).await.json();
        assert!(p["ocr_text"].as_str().is_some());
        let r = app
            .call(
                axum::http::Method::POST,
                &format!("/api/v1/posts/{id}/ocr"),
                Some(&t),
                None,
            )
            .await;
        assert_eq!(r.status, axum::http::StatusCode::OK);
        assert!(r.json()["ocr_text"].as_str().is_some());
        app.finish().await;
    }

    #[test]
    fn caps_length() {
        let line = "word ".repeat(1000);
        let raw = format!("{line}\n{line}\n{line}");
        let out = clean(&raw);
        let n = out.chars().count();
        assert!(n <= MAX_TEXT_CHARS && n > MAX_TEXT_CHARS - 5, "{n}");
        assert!(!out.ends_with('\n'));
        let multi = "é".repeat(MAX_TEXT_CHARS + 50);
        assert_eq!(clean(&multi).chars().count(), MAX_TEXT_CHARS);
    }
}
