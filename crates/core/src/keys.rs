//! Rules for API keys.

use thiserror::Error;

pub const MAX_API_KEY_NAME_CHARS: usize = 64;
pub const MAX_API_KEYS: usize = 50;
pub const MAX_API_KEY_DAYS: u32 = 3650;
/// Every API key starts with this; session tokens start with `sb_`.
pub const API_KEY_PREFIX: &str = "sbk_";
/// Characters of the token shown in key lists (`sbk_` and 8 hex digits).
pub const API_KEY_SHOWN_CHARS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiKeyError {
    #[error("give the key a name")]
    EmptyName,
    #[error("the name is longer than {MAX_API_KEY_NAME_CHARS} characters")]
    NameTooLong,
    #[error("the name contains a control character")]
    ControlChar,
    #[error("a key can last 1 to {MAX_API_KEY_DAYS} days")]
    BadExpiry,
}

/// Trims and checks a new key's name and lifetime.
pub fn clean_api_key(name: &str, expires_days: Option<u32>) -> Result<String, ApiKeyError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(ApiKeyError::EmptyName);
    }
    if name.chars().count() > MAX_API_KEY_NAME_CHARS {
        return Err(ApiKeyError::NameTooLong);
    }
    if name.chars().any(char::is_control) {
        return Err(ApiKeyError::ControlChar);
    }
    if expires_days.is_some_and(|d| !(1..=MAX_API_KEY_DAYS).contains(&d)) {
        return Err(ApiKeyError::BadExpiry);
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_names_and_lifetimes() {
        assert_eq!(
            clean_api_key("  backup   script ", None),
            Ok("backup script".into())
        );
        assert_eq!(clean_api_key("x", Some(1)), Ok("x".into()));
        assert_eq!(clean_api_key("x", Some(MAX_API_KEY_DAYS)), Ok("x".into()));
        assert_eq!(clean_api_key("   ", None), Err(ApiKeyError::EmptyName));
        assert_eq!(
            clean_api_key(&"n".repeat(MAX_API_KEY_NAME_CHARS + 1), None),
            Err(ApiKeyError::NameTooLong)
        );
        assert_eq!(
            clean_api_key("a\u{7}b", None),
            Err(ApiKeyError::ControlChar)
        );
        assert_eq!(clean_api_key("x", Some(0)), Err(ApiKeyError::BadExpiry));
        assert_eq!(
            clean_api_key("x", Some(MAX_API_KEY_DAYS + 1)),
            Err(ApiKeyError::BadExpiry)
        );
    }
}
