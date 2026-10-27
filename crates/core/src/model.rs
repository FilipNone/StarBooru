//! Request and response bodies of the HTTP API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Role::Admin),
            "user" => Some(Role::User),
            _ => None,
        }
    }
}

/// Content rating, following the Danbooru g/s/q/e scale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    #[default]
    General,
    Sensitive,
    Questionable,
    Explicit,
}

impl Rating {
    pub const ALL: [Rating; 4] = [
        Rating::General,
        Rating::Sensitive,
        Rating::Questionable,
        Rating::Explicit,
    ];

    pub fn to_db(self) -> i16 {
        match self {
            Rating::General => 0,
            Rating::Sensitive => 1,
            Rating::Questionable => 2,
            Rating::Explicit => 3,
        }
    }

    pub fn from_db(v: i16) -> Option<Self> {
        match v {
            0 => Some(Rating::General),
            1 => Some(Rating::Sensitive),
            2 => Some(Rating::Questionable),
            3 => Some(Rating::Explicit),
            _ => None,
        }
    }

    /// Accepts the full name or its first letter (`rating:e`).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "g" | "general" | "safe" => Some(Rating::General),
            "s" | "sensitive" => Some(Rating::Sensitive),
            "q" | "questionable" => Some(Rating::Questionable),
            "e" | "explicit" => Some(Rating::Explicit),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Rating::General => "general",
            Rating::Sensitive => "sensitive",
            Rating::Questionable => "questionable",
            Rating::Explicit => "explicit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Animation,
    Video,
}

impl MediaKind {
    pub fn to_db(self) -> i16 {
        match self {
            MediaKind::Image => 0,
            MediaKind::Animation => 1,
            MediaKind::Video => 2,
        }
    }

    pub fn from_db(v: i16) -> Option<Self> {
        match v {
            0 => Some(MediaKind::Image),
            1 => Some(MediaKind::Animation),
            2 => Some(MediaKind::Video),
            _ => None,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "image" | "img" => Some(MediaKind::Image),
            "animation" | "anim" | "gif" => Some(MediaKind::Animation),
            "video" | "vid" => Some(MediaKind::Video),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagCategory {
    #[default]
    General,
    Artist,
    Character,
    Copyright,
    Meta,
}

impl TagCategory {
    pub fn to_db(self) -> i16 {
        match self {
            TagCategory::General => 0,
            TagCategory::Artist => 1,
            TagCategory::Character => 2,
            TagCategory::Copyright => 3,
            TagCategory::Meta => 4,
        }
    }

    pub fn from_db(v: i16) -> Option<Self> {
        match v {
            0 => Some(TagCategory::General),
            1 => Some(TagCategory::Artist),
            2 => Some(TagCategory::Character),
            3 => Some(TagCategory::Copyright),
            4 => Some(TagCategory::Meta),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    /// False until the first account exists; that account becomes admin.
    pub has_users: bool,
    pub registration_open: bool,
    pub max_upload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub id: i64,
    pub kind: MediaKind,
    pub mime: String,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// Lowercase hex SHA-256 of the original file.
    pub sha256: String,
    pub rating: Rating,
    pub score: i32,
    /// The caller's own vote: -1, 0 or 1.
    pub my_vote: i16,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub deleted: bool,
    /// Signed, server-relative URL of the original file.
    pub file_url: String,
    /// Signed, server-relative URL of the thumbnail, if one exists.
    pub thumb_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostPage {
    pub posts: Vec<Post>,
    pub page: u32,
    pub limit: u32,
    pub has_more: bool,
}

/// Partial update: fields left `None` stay unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PostUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<Rating>,
    /// An empty string clears the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VoteRequest {
    /// -1, 0 (remove vote) or 1.
    pub value: i16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub category: TagCategory,
    pub post_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagUpdate {
    pub category: TagCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeResult {
    /// Posts permanently deleted.
    pub purged: u64,
}

/// Body of every non-2xx JSON response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    /// Stable machine-readable code, e.g. `not_found`, `duplicate`.
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_codes_round_trip() {
        for r in Rating::ALL {
            assert_eq!(Rating::from_db(r.to_db()), Some(r));
        }
        for k in [MediaKind::Image, MediaKind::Animation, MediaKind::Video] {
            assert_eq!(MediaKind::from_db(k.to_db()), Some(k));
        }
        assert_eq!(Rating::from_db(42), None);
        assert_eq!(TagCategory::from_db(-1), None);
    }

    #[test]
    fn enums_serialize_lowercase() {
        assert_eq!(
            serde_json::to_string(&Rating::Explicit).unwrap(),
            "\"explicit\""
        );
        assert_eq!(serde_json::to_string(&Role::Admin).unwrap(), "\"admin\"");
    }
}
