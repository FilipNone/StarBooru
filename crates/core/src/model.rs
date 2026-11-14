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

/// Content rating. `safe` is what other boorus call it; Danbooru's `general` means
/// the same and is still understood, in the API and in searches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    #[default]
    #[serde(alias = "general")]
    Safe,
    Sensitive,
    Questionable,
    Explicit,
}

impl Rating {
    pub const ALL: [Rating; 4] = [
        Rating::Safe,
        Rating::Sensitive,
        Rating::Questionable,
        Rating::Explicit,
    ];

    pub fn to_db(self) -> i16 {
        match self {
            Rating::Safe => 0,
            Rating::Sensitive => 1,
            Rating::Questionable => 2,
            Rating::Explicit => 3,
        }
    }

    pub fn from_db(v: i16) -> Option<Self> {
        match v {
            0 => Some(Rating::Safe),
            1 => Some(Rating::Sensitive),
            2 => Some(Rating::Questionable),
            3 => Some(Rating::Explicit),
            _ => None,
        }
    }

    /// Accepts the full name or Danbooru's letter (`rating:e`). `s` is sensitive,
    /// as there, so `safe` is written out or `g`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "safe" | "g" | "general" => Some(Rating::Safe),
            "s" | "sensitive" => Some(Rating::Sensitive),
            "q" | "questionable" => Some(Rating::Questionable),
            "e" | "explicit" => Some(Rating::Explicit),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Rating::Safe => "safe",
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
    /// Whether `POST /posts/import` is enabled.
    #[serde(default)]
    pub url_import: bool,
    /// The OpenID Connect provider's name, when sign-in through it is set up.
    #[serde(default)]
    pub oidc: Option<String>,
    /// Whether videos can be trimmed (the server has ffmpeg).
    #[serde(default)]
    pub video_editing: bool,
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

/// `PUT /auth/password`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PasswordChange {
    /// Ignored when the account has no password yet.
    #[serde(default)]
    pub current: String,
    pub new: String,
}

/// One signed-in device or app (`GET /auth/sessions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: i64,
    /// Like "Firefox on Linux"; empty for sessions from before this was kept.
    pub client: String,
    pub created_at: DateTime<Utc>,
    /// Updated at most every few minutes.
    pub last_used_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The session making this request.
    pub current: bool,
}

/// `DELETE /auth/sessions`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeResult {
    pub revoked: u64,
}

/// A long-lived token for scripts and other tools (`GET /keys`). It acts as
/// its owner, except that it can't manage sign-ins, the password or keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: i64,
    pub name: String,
    /// The start of the token, like `sbk_1a2b3c4d`, to tell keys apart.
    pub prefix: String,
    /// Only `GET` and `HEAD` requests.
    pub read_only: bool,
    pub created_at: DateTime<Utc>,
    /// Updated at most every few minutes; `None` until first used.
    pub last_used_at: Option<DateTime<Utc>>,
    /// `None`: never expires.
    pub expires_at: Option<DateTime<Utc>>,
}

/// `POST /keys`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewApiKey {
    pub name: String,
    #[serde(default)]
    pub read_only: bool,
    /// Days until it expires (1 to `MAX_API_KEY_DAYS`); `None` for never.
    #[serde(default)]
    pub expires_days: Option<u32>,
}

/// The answer to `POST /keys`. The token is shown only this once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedApiKey {
    pub key: ApiKey,
    pub token: String,
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
    /// Length of a video in milliseconds, when known.
    #[serde(default)]
    pub duration_ms: Option<i64>,
    /// Lowercase hex SHA-256 of the original file.
    pub sha256: String,
    pub rating: Rating,
    pub score: i32,
    /// The caller's own vote: -1, 0 or 1.
    pub my_vote: i16,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    /// The date in the file: EXIF for images (its wall clock, marked UTC, as
    /// EXIF has no time zone), the creation time for videos.
    #[serde(default)]
    pub file_date: Option<DateTime<Utc>>,
    /// The picture's main colours as `#rrggbb`, most common first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette: Vec<String>,
    pub deleted: bool,
    /// Signed, server-relative URL of the original file.
    pub file_url: String,
    /// Signed, server-relative URL of the thumbnail, if one exists.
    pub thumb_url: Option<String>,
    /// Signed, server-relative URL of a smaller JPEG copy for viewing, for
    /// big still images. The original stays the download.
    #[serde(default)]
    pub sample_url: Option<String>,
    /// Text found in the image by OCR. Only in the single-post response
    /// (`GET /posts/{id}`); `None` in lists and when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr_text: Option<String>,
    /// The tags with their category and section, for grouping them. Only in
    /// the single-post response; empty in lists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tag_details: Vec<PostTag>,
    /// The people in the picture, with their tags by slot. Only in the
    /// single-post response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub figures: Vec<crate::figure::Figure>,
}

/// One of a post's tags, as the viewer groups it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostTag {
    pub name: String,
    pub category: TagCategory,
    /// The top of the tag's parent chain (like `clothing` for `boots`), or
    /// the tag itself when it heads a tree. `None` for a tag on its own.
    #[serde(default)]
    pub section: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostPage {
    pub posts: Vec<Post>,
    pub page: u32,
    pub limit: u32,
    pub has_more: bool,
    /// Unknown tags the search corrected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrected: Vec<TagCorrection>,
}

/// An unknown tag, and the existing tag it probably meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagCorrection {
    pub from: String,
    pub to: String,
}

/// `POST /posts/import`: the server downloads `url` and adds it like an upload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportRequest {
    pub url: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub rating: Rating,
    /// Defaults to `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Whether AI tags the new post. `None` leaves it to the server's setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autotag: Option<bool>,
}

/// The size of a chunk in a chunked upload. A file no bigger than this goes in
/// one `POST /posts`. Small, because each chunk has to get through proxies that
/// cut off slow requests: Cloudflare over HTTP/3 was measured as low as 80 KB/s,
/// and gives up on a request after about a minute.
pub const UPLOAD_CHUNK_SIZE: u64 = 2 << 20;

/// `POST /uploads`: starts a chunked upload of a file of `size` bytes. What
/// goes on the post is given here, so a bad tag fails before anything is sent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UploadStart {
    pub size: u64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub rating: Rating,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Whether AI tags the new post. `None` leaves it to the server's setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autotag: Option<bool>,
}

/// The answer to [`UploadStart`]: where to send the chunks, and how to cut them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadSession {
    pub id: String,
    pub chunk_size: u64,
    pub chunks: u32,
}

/// How many chunks a file of `size` bytes is cut into.
pub fn upload_chunks(size: u64, chunk_size: u64) -> u32 {
    let Some(whole) = size.checked_div(chunk_size) else {
        return 0;
    };
    let partial = u64::from(size.checked_rem(chunk_size).is_some_and(|rest| rest != 0));
    u32::try_from(whole.saturating_add(partial)).unwrap_or(u32::MAX)
}

/// Where chunk `index` of a file of `size` bytes starts and how long it is, or
/// `None` past the end of the file.
pub fn upload_chunk_range(size: u64, chunk_size: u64, index: u32) -> Option<(u64, u64)> {
    let offset = u64::from(index).checked_mul(chunk_size)?;
    let left = size.checked_sub(offset).filter(|left| *left > 0)?;
    let len = left.min(chunk_size);
    (len > 0).then_some((offset, len))
}

/// Most addresses one `POST /imports` takes.
pub const MAX_IMPORT_BATCH: usize = 500;

/// `POST /imports`: addresses the server downloads in the background, so the
/// import goes on with every app closed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportBatch {
    pub items: Vec<ImportRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    /// The file was already here; `post_id` is that post when known.
    Duplicate,
    Failed,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Duplicate => "duplicate",
            JobState::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            JobState::Queued,
            JobState::Running,
            JobState::Done,
            JobState::Duplicate,
            JobState::Failed,
        ]
        .into_iter()
        .find(|j| j.as_str() == s)
    }

    /// Still waiting or running.
    pub fn is_active(self) -> bool {
        matches!(self, JobState::Queued | JobState::Running)
    }
}

/// One background import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportJob {
    pub id: i64,
    pub url: String,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub rating: Rating,
    pub state: JobState,
    pub post_id: Option<i64>,
    /// Why it failed, for people.
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// How far the download has got, while it is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<crate::transfer::Transfer>,
}

/// A search a user kept, listed on their gallery page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSearch {
    pub id: i64,
    pub name: String,
    pub query: String,
    pub created_at: DateTime<Utc>,
}

/// `POST /searches`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveSearchRequest {
    pub name: String,
    pub query: String,
}

/// Most posts one bulk edit may touch.
pub const MAX_BULK_POSTS: usize = 500;

/// `POST /posts/bulk`: one change applied to many posts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BulkEdit {
    pub ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<Rating>,
    /// Move the posts to the recycle bin. Can't be combined with edits.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub delete: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkResult {
    pub updated: u64,
    /// Posts left alone: missing, in the recycle bin, or not yours to delete.
    pub skipped: u64,
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

/// Default and longest window of the explore page, in days.
pub const DEFAULT_EXPLORE_DAYS: u32 = 7;
pub const MAX_EXPLORE_DAYS: u32 = 365;

/// `GET /explore`: what is popular lately, and ways to rediscover the
/// collection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Explore {
    pub days: u32,
    /// Most net upvotes cast in the window.
    pub trending: Vec<Post>,
    /// Most used on posts added in the window.
    pub tags: Vec<TagTrend>,
    /// Added on today's date (UTC) in earlier years, newest first.
    pub on_this_day: Vec<Post>,
    pub random: Vec<Post>,
    /// Unknown tags the search corrected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrected: Vec<TagCorrection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagTrend {
    pub tag: Tag,
    /// Posts added in the window that carry the tag.
    pub recent: i64,
}

/// Bits (of 64) two fingerprints may differ by and still count as
/// duplicates.
pub const DEFAULT_DUPLICATE_DISTANCE: u32 = 6;
pub const MAX_DUPLICATE_DISTANCE: u32 = 10;
/// Most posts listed per group, and accepted by resolve and dismiss.
pub const MAX_DUPLICATE_GROUP: usize = 50;

/// `GET /duplicates`: groups of posts that look alike.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Duplicates {
    pub distance: u32,
    /// Groups found; only the first `limit` are listed.
    pub total: u64,
    /// Live posts without a fingerprint yet (no thumbnail, or not hashed).
    pub unhashed: i64,
    pub groups: Vec<DuplicateGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Largest difference, in bits, between two linked posts.
    pub distance: u32,
    /// Posts in the group, which may exceed `posts.len()`.
    pub size: u32,
    /// Best first: most pixels, then the largest file, then the oldest.
    pub posts: Vec<Post>,
}

/// `POST /duplicates/resolve`: keep one post, move the others to the bin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveDuplicates {
    pub keep: i64,
    pub remove: Vec<i64>,
    /// Copy the removed posts' tags (and a missing source) to the kept one.
    #[serde(default)]
    pub merge_tags: bool,
}

/// `POST /duplicates/dismiss`: these posts are not duplicates of each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DismissDuplicates {
    pub ids: Vec<i64>,
}

/// `PATCH /tags/{name}`: fields left out stay as they are.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TagUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<TagCategory>,
    /// The parent tag's name (created if needed); `""` removes the parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// A tag with its aliases and implications (`GET /tags/{name}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagDetail {
    pub tag: Tag,
    /// Other names that resolve to this tag.
    pub aliases: Vec<String>,
    /// Tags added automatically to every post with this tag.
    pub implies: Vec<Tag>,
    /// Tags that add this one.
    pub implied_by: Vec<Tag>,
    /// The chain of parents, top first (`clothing`, `footwear` for `boots`).
    #[serde(default)]
    pub ancestors: Vec<Tag>,
    /// Tags directly under this one; searching this tag finds them too.
    #[serde(default)]
    pub children: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasRequest {
    pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplicationRequest {
    pub implies: String,
}

/// Sort order of the tag list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagOrder {
    /// Most used first.
    #[default]
    Count,
    Name,
}

/// The posts around one post in a search (`GET /posts/{id}/neighbors`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Neighbors {
    /// The post shown before it, if any.
    pub prev: Option<i64>,
    /// The post shown after it, if any.
    pub next: Option<i64>,
}

/// Posts asked for by `GET /posts/{id}/related` when no `limit` is given.
pub const DEFAULT_RELATED_LIMIT: u32 = 12;
/// The most posts `GET /posts/{id}/related` returns.
pub const MAX_RELATED_LIMIT: u32 = 48;

/// Posts like one post (`GET /posts/{id}/related`), most alike first: they
/// share its rarer tags or look like it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RelatedPosts {
    pub posts: Vec<Post>,
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
    fn cuts_a_file_into_chunks() {
        assert_eq!(upload_chunks(10, 5), 2);
        assert_eq!(upload_chunks(11, 5), 3);
        assert_eq!(upload_chunks(1, 5), 1);
        assert_eq!(upload_chunks(0, 5), 0);
        assert_eq!(upload_chunks(10, 0), 0);
        assert_eq!(upload_chunks(u64::MAX, 1), u32::MAX);

        assert_eq!(upload_chunk_range(11, 5, 0), Some((0, 5)));
        assert_eq!(upload_chunk_range(11, 5, 2), Some((10, 1)));
        assert_eq!(upload_chunk_range(11, 5, 3), None);
        assert_eq!(upload_chunk_range(10, 5, 2), None);
        assert_eq!(upload_chunk_range(10, 0, 0), None);
        assert_eq!(upload_chunk_range(u64::MAX, u64::MAX, u32::MAX), None);
    }

    #[test]
    fn safe_is_called_safe_and_general_still_means_it() {
        assert_eq!(serde_json::to_string(&Rating::Safe).unwrap(), "\"safe\"");
        assert_eq!(Rating::Safe.as_str(), "safe");
        for name in ["safe", "general"] {
            let json = format!("\"{name}\"");
            assert_eq!(serde_json::from_str::<Rating>(&json).unwrap(), Rating::Safe);
        }
        for name in ["safe", "SAFE", "g", "general"] {
            assert_eq!(Rating::parse(name), Some(Rating::Safe));
        }
        assert_eq!(Rating::parse("s"), Some(Rating::Sensitive));
    }

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

    #[test]
    fn job_states_round_trip() {
        for s in ["queued", "running", "done", "duplicate", "failed"] {
            let state = JobState::parse(s).unwrap();
            assert_eq!(state.as_str(), s);
            assert_eq!(serde_json::to_value(state).unwrap(), s);
        }
        assert_eq!(JobState::parse("cancelled"), None);
        assert!(JobState::Queued.is_active() && JobState::Running.is_active());
        assert!(!JobState::Done.is_active() && !JobState::Failed.is_active());
    }
}
