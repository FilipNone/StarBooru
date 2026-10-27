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

#[cfg(test)]
mod tests {
    use super::{escape_attr, format_duration, human_size};

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
}
