//! Shared StarBooru types used by the server and every client.
//!
//! This crate does no I/O so it compiles everywhere the clients run
//! (native, `wasm32-unknown-unknown`, Android).

pub mod admin;
pub mod agent;
pub mod edit;
pub mod keys;
pub mod model;
pub mod oidc;
pub mod query;
pub mod remote;
pub mod saved;
pub mod share;
pub mod tag;

pub use admin::{
    AdminUser, Backup, BackupSchedule, Backups, DayCount, PasswordReset, ServerSettings,
    ServerStats, Usage, UserUpdate, clean_settings,
};
pub use agent::describe_user_agent;
pub use edit::{Crop, EditError, EditKind, MediaEdit, Trim, edit_kind};
pub use keys::clean_api_key;
pub use model::*;
pub use oidc::{Identity, OidcBegin, OidcError, OidcFinish, OidcStart, SignInMethods};
pub use query::{Order, SearchQuery, SortKey, parse_query, with_order};
pub use remote::{
    BooruApi, BooruSite, RemoteError, RemotePage, RemotePost, RemoteSearch, SITES, remote_tags,
};
pub use saved::clean_saved_search;
pub use share::{NewShare, Share, ShareError, SharedPost, SharedView};
pub use tag::{normalize_tag, parse_tag_list};

/// Path prefix of the versioned JSON API.
pub const API_PREFIX: &str = "/api/v1";

/// Largest page a client may request from list endpoints.
pub const MAX_PAGE_LIMIT: u32 = 200;
/// Page size used when the client does not ask for one.
pub const DEFAULT_PAGE_LIMIT: u32 = 60;
/// Samples, the smaller copies the server makes of big images for viewing,
/// fit in this square (pixels).
pub const SAMPLE_SIZE: u32 = 2560;
