use std::path::Path;

use chrono::{DateTime, Utc};
use dioxus::html::FileData;
use starbooru_core::{Rating, is_media};

/// `1536` → `1.5 KB`.
pub fn human_size(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes.max(0) as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let name = UNITS.get(unit).copied().unwrap_or("B");
    if unit == 0 {
        format!("{value:.0} {name}")
    } else {
        format!("{value:.1} {name}")
    }
}

/// `m:ss`, or `h:mm:ss` from an hour. `None` for nonsense.
pub fn format_duration(ms: i64) -> Option<String> {
    if ms < 0 {
        return None;
    }
    let s = ms.saturating_add(500) / 1000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    Some(if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    })
}

/// Escapes text for a double-quoted HTML attribute.
pub fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Names the server's file types go by, which save reading the file.
const MEDIA_EXTENSIONS: [&str; 16] = [
    "jpg", "jpeg", "jfif", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "jxl", "mp4", "m4v",
    "webm", "mkv", "mov",
];

/// An image or video by its name; hidden files (`.DS_Store`, `._x.jpg`) are not.
pub fn is_media_name(name: &str) -> bool {
    !name.starts_with('.')
        && name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| MEDIA_EXTENSIONS.iter().any(|m| m.eq_ignore_ascii_case(ext)))
}

/// The images and videos among `files`, and how many others were left out. A
/// name can only say yes: a saved picture may be called just `anime`, so the
/// rest count by what they hold. The server checks every file again.
pub async fn media_files(files: Vec<FileData>) -> (Vec<FileData>, usize) {
    let total = files.len();
    let mut media = Vec::with_capacity(total);
    for file in files {
        let name = file.name();
        if is_media_name(&name)
            || (!name.starts_with('.') && file_head(&file).await.is_some_and(|h| is_media(&h)))
        {
            media.push(file);
        }
    }
    let skipped = total.saturating_sub(media.len());
    (media, skipped)
}

/// The start of a file, without reading the rest: `FileData`'s own stream reads
/// it all. `None` when it can't be read, as with a folder that was dropped.
#[cfg(target_arch = "wasm32")]
async fn file_head(file: &FileData) -> Option<Vec<u8>> {
    let head = file
        .inner()
        .downcast_ref::<web_sys::File>()?
        .slice_with_i32_and_i32(0, i32::from(starbooru_core::SNIFF_BYTES))
        .ok()?;
    let buffer = wasm_bindgen_futures::JsFuture::from(head.array_buffer())
        .await
        .ok()?;
    Some(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// The start of a file, without reading the rest: `FileData`'s own stream reads
/// it all. `None` when it can't be read, as with a folder that was dropped.
#[cfg(not(target_arch = "wasm32"))]
async fn file_head(file: &FileData) -> Option<Vec<u8>> {
    use tokio::io::AsyncReadExt;

    let mut head = Vec::new();
    tokio::fs::File::open(file.path())
        .await
        .ok()?
        .take(u64::from(starbooru_core::SNIFF_BYTES))
        .read_to_end(&mut head)
        .await
        .ok()?;
    Some(head)
}

/// The name of the folder holding `path`, as a tag (`Summer 2024/a.jpg` →
/// `summer_2024`).
pub fn folder_tag(path: &Path) -> Option<String> {
    let folder = path.parent()?.file_name()?.to_str()?;
    starbooru_core::normalize_tag(folder).ok()
}

/// The `http(s)://` addresses in pasted text, one per word, without repeats.
pub fn parse_urls(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        if (lower.starts_with("http://") || lower.starts_with("https://"))
            && !out.iter().any(|u| u == word)
        {
            out.push(word.to_owned());
        }
    }
    out
}

/// Splits a bulk tag edit like `cat -dog` into tags to add and to remove.
/// A tag in both lists is dropped from both, since the result is unclear.
pub fn split_tag_edit(text: &str) -> (Vec<String>, Vec<String>) {
    let mut add: Vec<String> = Vec::new();
    let mut remove: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let (list, name) = match word.strip_prefix('-') {
            Some(name) => (&mut remove, name),
            None => (&mut add, word),
        };
        let name = name.to_lowercase();
        if !name.is_empty() && !list.contains(&name) {
            list.push(name);
        }
    }
    let both: Vec<String> = add.iter().filter(|t| remove.contains(t)).cloned().collect();
    add.retain(|t| !both.contains(t));
    remove.retain(|t| !both.contains(t));
    (add, remove)
}

/// A rating's name in the current language.
pub fn rating_label(rating: Rating) -> String {
    match rating {
        Rating::General => t!("rating-general"),
        Rating::Sensitive => t!("rating-sensitive"),
        Rating::Questionable => t!("rating-questionable"),
        Rating::Explicit => t!("rating-explicit"),
    }
}

/// "1 post" or "N posts".
pub fn posts_count(n: u64) -> String {
    t!("posts-count", count = n)
}

/// "just now", "5 minutes ago", "yesterday", …, then a date.
pub fn time_ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - then).num_seconds();
    match secs {
        ..60 => t!("ago-now"),
        60..3_600 => t!("ago-minutes", count = secs / 60),
        3_600..86_400 => t!("ago-hours", count = secs / 3_600),
        86_400..172_800 => t!("ago-yesterday"),
        172_800..2_592_000 => t!("ago-days", count = secs / 86_400),
        _ => short_date(then),
    }
}

/// "17 Sep 2026", with the month in the current language.
pub fn short_date(date: DateTime<Utc>) -> String {
    use chrono::Datelike;
    let month = match date.month() {
        1 => t!("month-1"),
        2 => t!("month-2"),
        3 => t!("month-3"),
        4 => t!("month-4"),
        5 => t!("month-5"),
        6 => t!("month-6"),
        7 => t!("month-7"),
        8 => t!("month-8"),
        9 => t!("month-9"),
        10 => t!("month-10"),
        11 => t!("month-11"),
        _ => t!("month-12"),
    };
    t!(
        "date-short",
        day = date.day(),
        month = month,
        year = date.year()
    )
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;

    use bytes::Bytes;
    use dioxus::CapturedError;

    use super::*;

    #[test]
    fn says_how_long_ago() {
        let now = DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ago = |secs: i64| time_ago(now - chrono::Duration::seconds(secs), now);
        assert_eq!(ago(-30), "just now");
        assert_eq!(ago(59), "just now");
        assert_eq!(ago(60), "1 minute ago");
        assert_eq!(ago(7_199), "1 hour ago");
        assert_eq!(ago(7_200), "2 hours ago");
        assert_eq!(ago(90_000), "yesterday");
        assert_eq!(ago(3 * 86_400), "3 days ago");
        assert_eq!(ago(40 * 86_400), "17 Sep 2026");
    }

    #[test]
    fn splits_tag_edits() {
        let (add, remove) = split_tag_edit("  Cat -dog cat - -- sky -SKY night ");
        assert_eq!(add, ["cat", "night"]);
        assert_eq!(remove, ["dog", "-"]);
        assert_eq!(split_tag_edit(""), (vec![], vec![]));
        assert_eq!(posts_count(1), "1 post");
        assert_eq!(posts_count(0), "0 posts");
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(human_size(-4), "0 B");
        assert!(human_size(i64::MAX).ends_with(" TB"));
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0).as_deref(), Some("0:00"));
        assert_eq!(format_duration(1_499).as_deref(), Some("0:01"));
        assert_eq!(format_duration(65_000).as_deref(), Some("1:05"));
        assert_eq!(format_duration(3_600_000).as_deref(), Some("1:00:00"));
        assert_eq!(format_duration(-1), None);
        assert!(format_duration(i64::MAX).is_some());
    }

    #[test]
    fn escapes_attributes() {
        assert_eq!(escape_attr("/m.mp4?a=1&b=2"), "/m.mp4?a=1&amp;b=2");
        assert_eq!(escape_attr("\"><script>"), "&quot;&gt;&lt;script&gt;");
    }

    #[test]
    fn recognises_media_names() {
        assert!(is_media_name("a.JPG"));
        assert!(is_media_name("clip.final.webm"));
        assert!(!is_media_name("notes.txt"));
        assert!(!is_media_name("README"));
        assert!(!is_media_name("._a.jpg"));
        assert!(!is_media_name(".DS_Store"));
    }

    /// A file on disk, as the desktop app gets it.
    struct DiskFile(PathBuf);

    impl dioxus::html::NativeFileData for DiskFile {
        fn name(&self) -> String {
            self.0.file_name().unwrap().to_string_lossy().into_owned()
        }
        fn size(&self) -> u64 {
            self.0.metadata().map_or(0, |m| m.len())
        }
        fn last_modified(&self) -> u64 {
            0
        }
        fn path(&self) -> PathBuf {
            self.0.clone()
        }
        fn content_type(&self) -> Option<String> {
            None
        }
        fn read_bytes(&self) -> Pin<Box<dyn Future<Output = Result<Bytes, CapturedError>>>> {
            panic!("the type of a file comes from its start alone")
        }
        fn byte_stream(
            &self,
        ) -> Pin<Box<dyn futures_util::Stream<Item = Result<Bytes, CapturedError>> + Send>>
        {
            panic!("the type of a file comes from its start alone")
        }
        fn read_string(&self) -> Pin<Box<dyn Future<Output = Result<String, CapturedError>>>> {
            panic!("the type of a file comes from its start alone")
        }
        fn inner(&self) -> &dyn std::any::Any {
            &self.0
        }
    }

    #[tokio::test]
    async fn media_files_look_inside_what_their_name_does_not_settle() {
        let dir = std::env::temp_dir().join(format!("starbooru-media-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("album")).unwrap();
        let webp = b"RIFF\x24\0\0\0WEBPVP8 ".as_slice();
        for (name, bytes) in [
            ("anime", webp),
            ("1000", webp),
            ("saved.jpg_large", webp),
            ("named.jpg", b"not looked at".as_slice()),
            ("README", b"# Read me\n".as_slice()),
            ("notes.txt", b"nothing here".as_slice()),
            ("empty", b"".as_slice()),
            ("._hidden", webp),
        ] {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        let names = [
            "anime",
            "README",
            "named.jpg",
            "album",
            "notes.txt",
            "1000",
            "empty",
            "._hidden",
            "gone",
            "saved.jpg_large",
        ];
        let files = names
            .iter()
            .map(|n| FileData::new(DiskFile(dir.join(n))))
            .collect();

        let (media, skipped) = media_files(files).await;

        let kept: Vec<String> = media.iter().map(FileData::name).collect();
        assert_eq!(kept, ["anime", "named.jpg", "1000", "saved.jpg_large"]);
        assert_eq!(skipped, 6);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tags_from_folder_names() {
        assert_eq!(
            folder_tag(Path::new("Summer 2024/a.jpg")).as_deref(),
            Some("summer_2024")
        );
        assert_eq!(
            folder_tag(Path::new("/home/me/Pics/Cats/a.jpg")).as_deref(),
            Some("cats")
        );
        assert_eq!(folder_tag(Path::new("a.jpg")), None);
        assert_eq!(folder_tag(Path::new("")), None);
        assert_eq!(folder_tag(Path::new("-x/a.jpg")), None);
    }

    #[test]
    fn finds_urls_in_text() {
        assert_eq!(
            parse_urls(
                " https://a.example/1.png\nftp://x  HTTP://b.example/2 https://a.example/1.png word"
            ),
            ["https://a.example/1.png", "HTTP://b.example/2"]
        );
        assert!(parse_urls("").is_empty());
    }
}
