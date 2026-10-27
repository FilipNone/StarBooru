//! Share links: anyone with the link sees the chosen posts (media only, no
//! tags or account details) without signing in, until the link expires or
//! its owner removes it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::MediaKind;

/// Links one account may keep.
pub const MAX_SHARES: usize = 500;
/// Posts in one link.
pub const MAX_SHARE_POSTS: usize = 500;
pub const MAX_SHARE_TITLE_CHARS: usize = 100;
/// Longest lifetime a link can be given; it may also never expire.
pub const MAX_SHARE_DAYS: u32 = 365;
/// Characters in a link's token (lowercase hex).
pub const SHARE_TOKEN_CHARS: usize = 32;

/// `POST /shares`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewShare {
    /// In the order they are shown.
    pub post_ids: Vec<i64>,
    #[serde(default)]
    pub title: Option<String>,
    /// Days until the link stops working; `None` keeps it until removed.
    #[serde(default)]
    pub days: Option<u32>,
}

/// One of the caller's links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub id: i64,
    pub token: String,
    pub title: Option<String>,
    /// Posts the link was made with (some may have gone to the bin since).
    pub posts: u32,
    /// Signed, server-relative thumbnail of the first post still shown.
    pub cover_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Times the link was opened.
    pub views: i64,
    pub last_viewed_at: Option<DateTime<Utc>>,
}

/// A post as a link shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedPost {
    pub kind: MediaKind,
    pub mime: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Signed, server-relative URL of the original file.
    pub file_url: String,
    pub thumb_url: Option<String>,
    /// A smaller copy for viewing, as on `Post`.
    #[serde(default)]
    pub sample_url: Option<String>,
}

/// `GET /shared/{token}`: what a link shows, no account needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedView {
    pub title: Option<String>,
    /// The server's name.
    pub server: String,
    pub expires_at: Option<DateTime<Utc>>,
    /// Posts in the bin or erased since the link was made are left out.
    pub posts: Vec<SharedPost>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShareError {
    #[error("pick at least one post to share")]
    NoPosts,
    #[error("a link can hold at most {MAX_SHARE_POSTS} posts")]
    TooManyPosts,
    #[error("the title is longer than {MAX_SHARE_TITLE_CHARS} characters")]
    TitleTooLong,
    #[error("the title contains a control character")]
    ControlChar,
    #[error("a link lasts 1 to {MAX_SHARE_DAYS} days, or until removed")]
    BadDays,
}

impl NewShare {
    /// Drops repeated posts (keeping the first place), trims the title
    /// (empty is none) and checks the limits.
    pub fn clean(self) -> Result<NewShare, ShareError> {
        let mut post_ids = Vec::with_capacity(self.post_ids.len().min(MAX_SHARE_POSTS));
        for id in self.post_ids {
            if !post_ids.contains(&id) {
                if post_ids.len() == MAX_SHARE_POSTS {
                    return Err(ShareError::TooManyPosts);
                }
                post_ids.push(id);
            }
        }
        if post_ids.is_empty() {
            return Err(ShareError::NoPosts);
        }
        let title = self
            .title
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        if let Some(t) = &title {
            if t.chars().count() > MAX_SHARE_TITLE_CHARS {
                return Err(ShareError::TitleTooLong);
            }
            if t.chars().any(char::is_control) {
                return Err(ShareError::ControlChar);
            }
        }
        if self.days.is_some_and(|d| d == 0 || d > MAX_SHARE_DAYS) {
            return Err(ShareError::BadDays);
        }
        Ok(NewShare {
            post_ids,
            title,
            days: self.days,
        })
    }
}

/// The page a link opens, relative to the server (which serves the web app).
pub fn share_path(token: &str) -> String {
    format!("/s/{token}")
}

/// Whether `token` looks like a link token, before asking the database.
pub fn is_share_token(token: &str) -> bool {
    token.len() == SHARE_TOKEN_CHARS
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(ids: Vec<i64>, title: Option<&str>, days: Option<u32>) -> NewShare {
        NewShare {
            post_ids: ids,
            title: title.map(str::to_owned),
            days,
        }
    }

    #[test]
    fn cleans_new_shares() {
        let clean = share(vec![3, 1, 3, 2, 1], Some("  Trip  "), Some(7))
            .clean()
            .unwrap();
        assert_eq!(clean.post_ids, [3, 1, 2]);
        assert_eq!(clean.title.as_deref(), Some("Trip"));
        assert_eq!(clean.days, Some(7));
        assert_eq!(
            share(vec![1], Some("  "), None).clean().unwrap().title,
            None
        );
        assert_eq!(share(vec![], None, None).clean(), Err(ShareError::NoPosts));
        let many: Vec<i64> = (0..=500).collect();
        assert_eq!(
            share(many, None, None).clean(),
            Err(ShareError::TooManyPosts)
        );
        // Repeats don't count towards the limit.
        let repeated: Vec<i64> = (0..500).chain(0..500).collect();
        assert!(share(repeated, None, None).clean().is_ok());
        let long = "x".repeat(101);
        assert_eq!(
            share(vec![1], Some(&long), None).clean(),
            Err(ShareError::TitleTooLong)
        );
        assert_eq!(
            share(vec![1], Some("a\nb"), None).clean(),
            Err(ShareError::ControlChar)
        );
        assert_eq!(
            share(vec![1], None, Some(0)).clean(),
            Err(ShareError::BadDays)
        );
        assert_eq!(
            share(vec![1], None, Some(366)).clean(),
            Err(ShareError::BadDays)
        );
        assert!(share(vec![1], None, Some(365)).clean().is_ok());
    }

    #[test]
    fn tokens() {
        assert!(is_share_token("0123456789abcdef0123456789abcdef"));
        assert!(!is_share_token("0123456789ABCDEF0123456789abcdef"));
        assert!(!is_share_token("0123456789abcdef"));
        assert!(!is_share_token("../../etc/passwd/xxxxxxxxxxxxxxxx"));
        assert_eq!(share_path("ab"), "/s/ab");
    }
}
