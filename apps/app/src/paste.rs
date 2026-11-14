//! Ctrl+V on the Upload page: pictures and files on the clipboard, and
//! links. The browser hands its files over as they are. A webview's files
//! have no path, so their bytes come across and are kept in a folder of the
//! app's until they are uploaded; files copied in a file manager come as
//! `file://` addresses, read where they are.

use dioxus::html::FileData;
use dioxus::prelude::*;
use serde::Deserialize;

/// What a paste carried.
#[derive(Debug, Default, Deserialize)]
pub struct Pasted {
    /// How many files were on the clipboard.
    #[serde(default)]
    pub files: usize,
    /// Their bytes, for a webview, which can't name them otherwise.
    #[serde(default)]
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub read: Vec<PastedFile>,
    /// Text on the clipboard: links, or the paths of copied files.
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct PastedFile {
    pub name: String,
    /// Base64, or nothing for a file over [`MAX_PASTE_BYTES`].
    pub data: Option<String>,
}

/// The largest file a webview passes over by value. Bigger ones are best
/// copied as files in a file manager, which pastes their paths.
pub const MAX_PASTE_BYTES: u64 = 64 << 20;

/// Listens for pastes on the window, except into a field or while a popup
/// is open, which have their own. Sends a [`Pasted`].
const WATCH_PASTE: &str = r#"
window.__sbPaste?.stop();
const paste = async (e) => {
  if (e.target?.closest?.("input, textarea, select, [contenteditable]")) return;
  if (document.querySelector("[role=dialog]")) return;
  const data = e.clipboardData;
  if (!data) return;
  const files = [...data.files];
  const text = data.getData("text/uri-list") || data.getData("text/plain") || "";
  if (!files.length && !text.trim()) return;
  e.preventDefault();
  window.__sbPasted = files;
  const read = [];
  if (READ_BYTES) {
    for (const f of files) {
      if (f.size > MAX_BYTES) { read.push({ name: f.name, data: null }); continue; }
      const bytes = new Uint8Array(await f.arrayBuffer());
      let bin = "";
      for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      read.push({ name: f.name, data: btoa(bin) });
    }
  }
  dioxus.send({ files: files.length, read, text });
};
addEventListener("paste", paste);
window.__sbPaste = { stop() { removeEventListener("paste", paste); } };
"#;
const WATCH_PASTE_OFF: &str =
    "window.__sbPaste?.stop(); window.__sbPaste = null; window.__sbPasted = null;";

/// Starts listening for pastes; they arrive through the returned eval.
pub fn watch() -> document::Eval {
    let script = WATCH_PASTE
        .replace(
            "READ_BYTES",
            if cfg!(target_arch = "wasm32") {
                "false"
            } else {
                "true"
            },
        )
        .replace("MAX_BYTES", &MAX_PASTE_BYTES.to_string());
    document::eval(&script)
}

/// Stops listening.
pub fn unwatch() {
    document::eval(WATCH_PASTE_OFF);
}

/// The files of a paste, and how many were too big to take.
#[cfg(target_arch = "wasm32")]
pub async fn files(pasted: &Pasted) -> (Vec<FileData>, usize) {
    use web_sys::wasm_bindgen::JsCast;

    let _ = pasted;
    let Some(window) = web_sys::window() else {
        return (Vec::new(), 0);
    };
    let list = js_sys::Reflect::get(&window, &"__sbPasted".into()).unwrap_or_default();
    let _ = js_sys::Reflect::set(
        &window,
        &"__sbPasted".into(),
        &web_sys::wasm_bindgen::JsValue::NULL,
    );
    let Ok(list) = list.dyn_into::<js_sys::Array>() else {
        return (Vec::new(), 0);
    };
    let mut files = Vec::new();
    for item in list.iter() {
        let Ok(file) = item.dyn_into::<web_sys::File>() else {
            continue;
        };
        let Ok(reader) = web_sys::FileReader::new() else {
            continue;
        };
        files.push(FileData::new(dioxus::web::WebFileData::new(file, reader)));
    }
    (files, 0)
}

/// The files of a paste, and how many were too big to take.
#[cfg(not(target_arch = "wasm32"))]
pub async fn files(pasted: &Pasted) -> (Vec<FileData>, usize) {
    use base64::Engine;

    let mut files = Vec::new();
    let mut too_big = 0usize;
    for path in pasted.text.lines().filter_map(file_uri_path) {
        if let Some(file) = native::PathFile::open(path).await {
            files.push(FileData::new(file));
        }
    }
    if pasted.read.is_empty() {
        return (files, too_big);
    }
    let Some(dir) = native::paste_dir().await else {
        return (files, too_big);
    };
    for (index, item) in pasted.read.iter().enumerate() {
        let Some(data) = &item.data else {
            too_big = too_big.saturating_add(1);
            continue;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            continue;
        };
        let name = safe_name(&item.name, index);
        if let Some(file) = native::keep(&dir, index, &name, &bytes).await {
            files.push(FileData::new(file));
        }
    }
    (files, too_big)
}

/// A file name that stays inside the folder it is written to.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn safe_name(name: &str, index: usize) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches('.');
    if base.is_empty() {
        format!("pasted-{index}.png")
    } else {
        base.chars().take(120).collect()
    }
}

/// The path a `file://` address names, for files copied in a file manager.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn file_uri_path(line: &str) -> Option<std::path::PathBuf> {
    let rest = line.trim().strip_prefix("file://")?;
    // `file:///home/a.png`, or `file://localhost/home/a.png`.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(rest)?;
    // `file:///C:/a.png` on Windows.
    #[cfg(windows)]
    let decoded = decoded
        .strip_prefix('/')
        .filter(|p| p.get(1..2) == Some(":"))
        .map(str::to_owned)
        .unwrap_or(decoded);
    Some(std::path::PathBuf::from(decoded))
}

/// `%20` and the like, as UTF-8. `None` for a broken escape.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = s.get(i.checked_add(1)?..i.checked_add(3)?)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i = i.checked_add(3)?;
        } else {
            out.push(b);
            i = i.checked_add(1)?;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::path::{Path, PathBuf};
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU64, Ordering};

    use bytes::Bytes;
    use dioxus::core::CapturedError;
    use dioxus::html::NativeFileData;

    /// Pasted files older than this are deleted when the next paste comes.
    const KEEP_SECS: u64 = 24 * 60 * 60;

    /// Tells two pastes in the same millisecond apart.
    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// A file on disk, as the uploader reads it: by its path.
    pub struct PathFile {
        path: PathBuf,
        size: u64,
        modified: u64,
    }

    impl PathFile {
        /// A regular file at `path`, or `None`.
        pub async fn open(path: PathBuf) -> Option<Self> {
            let meta = tokio::fs::metadata(&path).await.ok()?;
            if !meta.is_file() {
                return None;
            }
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            Some(Self {
                path,
                size: meta.len(),
                modified,
            })
        }
    }

    type Read<T> = Pin<Box<dyn std::future::Future<Output = Result<T, CapturedError>> + 'static>>;

    impl NativeFileData for PathFile {
        fn name(&self) -> String {
            self.path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        }

        fn size(&self) -> u64 {
            self.size
        }

        fn last_modified(&self) -> u64 {
            self.modified
        }

        fn path(&self) -> PathBuf {
            self.path.clone()
        }

        fn content_type(&self) -> Option<String> {
            None
        }

        fn read_bytes(&self) -> Read<Bytes> {
            let path = self.path.clone();
            Box::pin(async move { Ok(Bytes::from(tokio::fs::read(&path).await?)) })
        }

        fn byte_stream(
            &self,
        ) -> Pin<Box<dyn futures_util::Stream<Item = Result<Bytes, CapturedError>> + 'static + Send>>
        {
            let path = self.path.clone();
            Box::pin(futures_util::stream::once(async move {
                Ok(Bytes::from(tokio::fs::read(&path).await?))
            }))
        }

        fn read_string(&self) -> Read<String> {
            let path = self.path.clone();
            Box::pin(async move { Ok(tokio::fs::read_to_string(&path).await?) })
        }

        fn inner(&self) -> &dyn std::any::Any {
            &self.path
        }
    }

    /// Where pasted files wait for their upload, after clearing out old ones.
    pub async fn paste_dir() -> Option<PathBuf> {
        #[cfg(target_os = "android")]
        let root = crate::system::android::cache_dir();
        #[cfg(not(target_os = "android"))]
        let root = directories::ProjectDirs::from("eu", "filipnone", "StarBooru")
            .map(|d| d.cache_dir().to_path_buf());
        let dir = root?.join("pasted");
        tokio::fs::create_dir_all(&dir).await.ok()?;
        forget_old(&dir).await;
        Some(dir)
    }

    /// Deletes pastes from earlier days. Each sits in a folder of its own.
    async fn forget_old(dir: &Path) {
        let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
            return;
        };
        let now = std::time::SystemTime::now();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let old = entry
                .metadata()
                .await
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|age| age.as_secs() > KEEP_SECS);
            if old {
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
    }

    /// Writes one pasted file under its own name, in a folder of its own.
    pub async fn keep(dir: &Path, index: usize, name: &str, bytes: &[u8]) -> Option<PathFile> {
        let stamp = chrono::Utc::now().timestamp_millis();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let folder = dir.join(format!("{stamp}-{n}-{index}"));
        tokio::fs::create_dir_all(&folder).await.ok()?;
        let path = folder.join(name);
        tokio::fs::write(&path, bytes).await.ok()?;
        PathFile::open(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_name_stays_in_its_folder() {
        assert_eq!(safe_name("image.png", 0), "image.png");
        assert_eq!(safe_name("../../etc/passwd", 0), "passwd");
        assert_eq!(safe_name("a\\b\\c.jpg", 0), "c.jpg");
        assert_eq!(safe_name("..", 3), "pasted-3.png");
        assert_eq!(safe_name("", 1), "pasted-1.png");
        assert_eq!(safe_name(".hidden.png", 0), "hidden.png");
        assert_eq!(safe_name(&"x".repeat(500), 0).len(), 120);
    }

    #[test]
    fn copied_files_come_as_their_paths() {
        assert_eq!(
            file_uri_path("file:///home/me/My%20Pictures/a.png"),
            Some("/home/me/My Pictures/a.png".into())
        );
        assert_eq!(
            file_uri_path("file://localhost/tmp/b.jpg"),
            Some("/tmp/b.jpg".into())
        );
        assert_eq!(file_uri_path("https://example.com/a.png"), None);
        assert_eq!(file_uri_path("file://server/share/a.png"), None);
        // A broken escape is no path at all.
        assert_eq!(file_uri_path("file:///a%2"), None);
        assert_eq!(file_uri_path("file:///a%zz"), None);
        assert_eq!(percent_decode("%E2%98%85"), Some("★".into()));
    }

    #[test]
    fn a_paste_is_left_alone_inside_a_field_or_a_popup() {
        assert!(WATCH_PASTE.contains(r#"closest?.("input, textarea, select, [contenteditable]")"#));
        assert!(WATCH_PASTE.contains("[role=dialog]"));
        assert!(WATCH_PASTE_OFF.contains("__sbPaste?.stop()"));
    }

    #[test]
    fn a_paste_arrives_whole() {
        let p: Pasted =
            serde_json::from_str(r#"{"files":1,"read":[{"name":"a.png","data":null}],"text":""}"#)
                .unwrap();
        assert_eq!(p.files, 1);
        assert!(p.read.first().is_some_and(|f| f.data.is_none()));
        let empty: Pasted = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.files, 0);
    }
}
