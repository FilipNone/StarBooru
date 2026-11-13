use std::path::Path;

use chrono::{DateTime, Utc};
use dioxus::html::FileData;
use dioxus::prelude::document;
use starbooru_core::{Rating, is_media};

/// Waits `ms` milliseconds on the page's own timer, which works the same in
/// the browser and the webviews.
pub async fn sleep_ms(ms: u32) {
    let script = format!("await new Promise((r) => setTimeout(r, {ms})); return true;");
    let _ = document::eval(&script).join::<bool>().await;
}

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

/// What to say beside a transfer's bar: `42% · 1.2 MB/s`, or the bytes so
/// far when the size isn't known. The speed is left out until it is measured.
pub fn transfer_label(progress: &starbooru_core::Transfer) -> String {
    let size = |bytes: u64| human_size(i64::try_from(bytes).unwrap_or(i64::MAX));
    let how_far = match progress.percent() {
        Some(percent) => format!("{percent}%"),
        None => size(progress.done),
    };
    if progress.speed == 0 {
        how_far
    } else {
        format!("{how_far} · {}/s", size(progress.speed))
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

/// A piece of a file, without reading the rest.
#[cfg(target_arch = "wasm32")]
pub async fn read_range(file: &FileData, offset: u64, len: u64) -> Result<bytes::Bytes, String> {
    let piece = file
        .inner()
        .downcast_ref::<web_sys::File>()
        .ok_or_else(|| "not a file".to_owned())?
        .slice_with_f64_and_f64(offset as f64, offset.saturating_add(len) as f64)
        .map_err(|e| format!("{e:?}"))?;
    let buffer = wasm_bindgen_futures::JsFuture::from(piece.array_buffer())
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec().into())
}

/// A piece of a file, without reading the rest.
#[cfg(not(target_arch = "wasm32"))]
pub async fn read_range(file: &FileData, offset: u64, len: u64) -> Result<bytes::Bytes, String> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut opened = tokio::fs::File::open(file.path())
        .await
        .map_err(|e| e.to_string())?;
    opened
        .seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(|e| e.to_string())?;
    let mut piece = Vec::new();
    opened
        .take(len)
        .read_to_end(&mut piece)
        .await
        .map_err(|e| e.to_string())?;
    Ok(piece.into())
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

/// The address of the first picture in a piece of HTML, as a drag out of a
/// browser carries it.
fn img_src(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let tag = lower.find("<img")?;
    let end = tag + lower.get(tag..)?.find('>')?;
    let attrs = html.get(tag..end)?;
    let at = attrs.to_ascii_lowercase().find("src=")? + 4;
    let value = attrs.get(at..)?.trim_start();
    let src = match value.chars().next()? {
        quote @ ('"' | '\'') => value.get(1..)?.split(quote).next()?,
        _ => value.split_whitespace().next()?,
    };
    let src = src.trim().replace("&amp;", "&");
    let src = match src.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => src,
    };
    parse_urls(&src).into_iter().next()
}

/// What to import for a link or a picture dragged out of a browser, from the
/// forms a drop carries it in. A link to a post on a booru is kept, as the
/// server finds the original and its tags from it; otherwise a dragged
/// picture counts before the link around it.
pub fn drop_urls(uri_list: &str, html: &str, text: &str) -> Vec<String> {
    let listed: String = uri_list
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let mut links = parse_urls(&listed);
    if links.is_empty() {
        links = parse_urls(text);
    }
    let is_post = |url: &String| starbooru_core::post_link(url, &[]).is_some();
    match (links.as_slice(), img_src(html)) {
        ([], Some(picture)) => vec![picture],
        ([link], Some(picture)) if !is_post(link) => vec![picture],
        _ => links,
    }
}

/// Whether a drop is taken as addresses and not as files. A browser may hand
/// over a copy of a dragged picture as a file; the address is better when it
/// leads to the post, with its original and its tags. The desktop app lists
/// the files of the last drag again when there are none, so there an address
/// always wins.
pub fn drop_is_urls(files: usize, urls: &[String]) -> bool {
    !urls.is_empty()
        && (files == 0
            || cfg!(feature = "desktop")
            || urls
                .iter()
                .any(|url| starbooru_core::post_link(url, &[]).is_some()))
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
        Rating::Safe => t!("rating-safe"),
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

    #[tokio::test]
    async fn reads_a_piece_of_a_file() {
        let dir = std::env::temp_dir().join(format!("starbooru-range-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.mp4");
        std::fs::write(&path, b"0123456789").unwrap();
        let file = FileData::new(DiskFile(path));

        assert_eq!(read_range(&file, 0, 4).await.unwrap().as_ref(), b"0123");
        assert_eq!(read_range(&file, 7, 3).await.unwrap().as_ref(), b"789");
        // Past the end there is less than was asked for, which the client notices.
        assert_eq!(read_range(&file, 8, 5).await.unwrap().as_ref(), b"89");
        assert_eq!(read_range(&file, 20, 5).await.unwrap().as_ref(), b"");

        let gone = FileData::new(DiskFile(dir.join("gone")));
        assert!(read_range(&gone, 0, 1).await.is_err());
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

    #[test]
    fn says_how_far_and_how_fast() {
        let t = |done, total, speed| starbooru_core::Transfer { done, total, speed };
        assert_eq!(transfer_label(&t(0, Some(1000), 0)), "0%");
        assert_eq!(
            transfer_label(&t(420, Some(1000), 1_288_490)),
            "42% · 1.2 MB/s"
        );
        assert_eq!(transfer_label(&t(1000, Some(1000), 512)), "100% · 512 B/s");
        // No size from the site: the bytes so far.
        assert_eq!(transfer_label(&t(3_565_158, None, 0)), "3.4 MB");
        assert_eq!(
            transfer_label(&t(3_565_158, None, 2048)),
            "3.4 MB · 2.0 KB/s"
        );
    }

    #[test]
    fn reads_what_a_browser_drag_carries() {
        let post = "https://gelbooru.com/index.php?page=post&s=view&id=15250239";
        let thumb = "https://img3.gelbooru.com/thumbnails/aa/bb/thumbnail_x.jpg";
        let wrapped =
            format!(r#"<meta charset="utf-8"><a href="{post}"><img alt="x" src="{thumb}"></a>"#);
        // A thumbnail in a list of posts: the post it links to.
        assert_eq!(drop_urls(post, &wrapped, post), [post]);
        // A picture inside any other link: the picture.
        let wrapped =
            format!("<a href='https://blog.example/p/1'><IMG SRC='{thumb}?a=1&amp;b=2' ></a>");
        assert_eq!(
            drop_urls("https://blog.example/p/1", &wrapped, ""),
            [format!("{thumb}?a=1&b=2")]
        );
        // A bare picture, as Firefox and Chromium list it.
        let alone = format!(r#"<img src="{thumb}" width="150">"#);
        assert_eq!(
            drop_urls(&format!("# a comment\r\n{thumb}\r\n"), &alone, thumb),
            [thumb]
        );
        assert_eq!(
            drop_urls("", "<img src=//cdn.example/a.png>", ""),
            ["https://cdn.example/a.png"]
        );
        // A plain link, or just its text.
        assert_eq!(
            drop_urls("https://a.example/x.png", "", ""),
            ["https://a.example/x.png"]
        );
        assert_eq!(
            drop_urls("", "", " https://a.example/x.png "),
            ["https://a.example/x.png"]
        );
        // Nothing to import: files from a file manager, text, a picture on this page.
        assert!(drop_urls("file:///home/me/a.png", "", "").is_empty());
        assert!(drop_urls("", "<p>hello</p>", "hello").is_empty());
        assert!(drop_urls("", r#"<img src="data:image/png;base64,AA">"#, "").is_empty());
        assert!(drop_urls("", "<img", "").is_empty());
    }

    #[test]
    fn an_address_beats_a_file_only_when_it_leads_to_the_post() {
        let post = ["https://danbooru.donmai.us/posts/1".to_owned()];
        let other = ["https://blog.example/a.png".to_owned()];
        assert!(drop_is_urls(0, &other));
        assert!(drop_is_urls(1, &post));
        assert!(!drop_is_urls(0, &[]));
        assert!(!drop_is_urls(2, &[]));
        // The desktop app's files may be left over from an earlier drag.
        assert_eq!(drop_is_urls(1, &other), cfg!(feature = "desktop"));
    }
}
