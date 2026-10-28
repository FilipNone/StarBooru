//! Shared StarBooru types used by the server and every client.
//!
//! This crate does no I/O so it compiles everywhere the clients run
//! (native, `wasm32-unknown-unknown`, Android).

pub mod admin;
pub mod agent;
pub mod ai;
pub mod autotag;
pub mod edit;
pub mod keys;
pub mod local_ai;
pub mod model;
pub mod oidc;
pub mod panels;
pub mod query;
pub mod remote;
pub mod saved;
pub mod share;
pub mod tag;
pub mod vocab;

pub use admin::{
    AdminUser, Backup, BackupSchedule, Backups, DayCount, PasswordReset, ServerSettings,
    ServerStats, Usage, UserUpdate, clean_settings,
};
pub use agent::describe_user_agent;
pub use ai::{
    AiRoute, AiSettings, AiSettingsUpdate, AiStatus, LocalAiSettings, TagSuggestion,
    TagSuggestions, TranslateRequest, Translation, clean_ai_settings, clean_language,
    clean_translation, parse_suggestions, translate_prompt,
};
pub use autotag::{META_TAGS, MediaFacts, meta_tags};
pub use edit::{Crop, EditError, EditKind, MediaEdit, Trim, edit_kind};
pub use keys::clean_api_key;
pub use local_ai::{
    Fit, FitCheck, Gpu, GpuKind, Hardware, LocalAiStatus, LocalModel, LocalRuntime,
    LocalTestResult, LocalTier, Processor, PullProgress, PullRequest, TierCheck,
};
pub use model::*;
pub use oidc::{Identity, OidcBegin, OidcError, OidcFinish, OidcStart, SignInMethods};
pub use panels::{PanelTranslation, TextBlock, panel_prompt, parse_panel_blocks};
pub use query::{Order, SearchQuery, SortKey, parse_query, with_order};
pub use remote::{
    BooruApi, BooruSite, RemoteError, RemotePage, RemotePost, RemoteSearch, SITES, remote_tags,
};
pub use saved::clean_saved_search;
pub use share::{NewShare, Share, ShareError, SharedPost, SharedView};
pub use tag::{normalize_tag, parse_tag_list};
pub use vocab::{SECTIONS, VOCAB, VOCAB_VERSION, split_category};

/// Path prefix of the versioned JSON API.
pub const API_PREFIX: &str = "/api/v1";

/// Largest page a client may request from list endpoints.
pub const MAX_PAGE_LIMIT: u32 = 200;
/// Page size used when the client does not ask for one.
pub const DEFAULT_PAGE_LIMIT: u32 = 60;
/// Samples, the smaller copies the server makes of big images for viewing,
/// fit in this square (pixels).
pub const SAMPLE_SIZE: u32 = 2560;
