//! Tag name rules shared by server and clients.

use thiserror::Error;

pub const MAX_TAG_CHARS: usize = 128;
pub const MAX_TAGS_PER_POST: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TagError {
    #[error("tag is empty")]
    Empty,
    #[error("tag is longer than {MAX_TAG_CHARS} characters")]
    TooLong,
    #[error("tag `{0}` may not start with `-` or `~`")]
    BadPrefix(String),
    #[error("tag contains a control character")]
    ControlChar,
    #[error("too many tags (max {MAX_TAGS_PER_POST})")]
    TooMany,
}

/// Canonical form: trimmed, lowercase, inner whitespace collapsed to one `_`.
pub fn normalize_tag(raw: &str) -> Result<String, TagError> {
    let mut out = String::with_capacity(raw.len());
    let mut pending_underscore = false;
    for c in raw.trim().chars() {
        if c.is_control() {
            return Err(TagError::ControlChar);
        }
        if c.is_whitespace() || c == '_' {
            pending_underscore = !out.is_empty();
            continue;
        }
        if pending_underscore {
            out.push('_');
            pending_underscore = false;
        }
        out.extend(c.to_lowercase());
    }
    if out.is_empty() {
        return Err(TagError::Empty);
    }
    if out.chars().count() > MAX_TAG_CHARS {
        return Err(TagError::TooLong);
    }
    if out.starts_with('-') || out.starts_with('~') {
        return Err(TagError::BadPrefix(out));
    }
    Ok(out)
}

/// Splits a whitespace-separated tag string, normalises and de-duplicates it.
pub fn parse_tag_list(input: &str) -> Result<Vec<String>, TagError> {
    normalize_tags(input.split_whitespace())
}

/// Normalises and de-duplicates tags, keeping first-seen order.
pub fn normalize_tags<'a>(
    tags: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<String>, TagError> {
    let mut out: Vec<String> = Vec::new();
    for raw in tags {
        let tag = normalize_tag(raw)?;
        if !out.contains(&tag) {
            if out.len() >= MAX_TAGS_PER_POST {
                return Err(TagError::TooMany);
            }
            out.push(tag);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize_tag("  Blue Sky ").unwrap(), "blue_sky");
        assert_eq!(normalize_tag("a__b").unwrap(), "a_b");
        assert_eq!(normalize_tag("_lead_").unwrap(), "lead");
        assert_eq!(normalize_tag("ÄÖ").unwrap(), "äö");
    }

    #[test]
    fn rejects_bad_tags() {
        assert_eq!(normalize_tag("   "), Err(TagError::Empty));
        assert!(matches!(normalize_tag("-x"), Err(TagError::BadPrefix(_))));
        assert_eq!(normalize_tag("a\u{0}b"), Err(TagError::ControlChar));
        assert_eq!(normalize_tag(&"x".repeat(129)), Err(TagError::TooLong));
    }

    #[test]
    fn list_dedupes() {
        assert_eq!(parse_tag_list("b a B").unwrap(), vec!["b", "a"]);
        let many: Vec<String> = (0..=MAX_TAGS_PER_POST).map(|i| format!("t{i}")).collect();
        assert_eq!(parse_tag_list(&many.join(" ")), Err(TagError::TooMany));
    }
}
