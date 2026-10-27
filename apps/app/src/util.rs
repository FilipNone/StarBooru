use std::path::Path;

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

/// File types the server accepts, to skip other files in an imported folder.
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

/// "1 post" or "N posts".
pub fn posts_count(n: u64) -> String {
    match n {
        1 => "1 post".to_owned(),
        n => format!("{n} posts"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
