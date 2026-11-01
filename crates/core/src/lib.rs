//! Shared StarBooru types used by the server and every client.
//!
//! This crate does no I/O so it compiles everywhere the clients run
//! (native, `wasm32-unknown-unknown`, Android).

pub mod admin;
pub mod agent;
pub mod ai;
pub mod ai_catalog;
mod ai_files;
pub mod ai_worker;
pub mod autotag;
pub mod color;
pub mod edit;
pub mod figure;
pub mod filter;
pub mod keys;
pub mod library;
pub mod local_ai;
pub mod media;
pub mod model;
pub mod oidc;
pub mod panels;
pub mod query;
pub mod remote;
pub mod saved;
pub mod share;
pub mod tag;
pub mod typo;
pub mod vocab;

pub use admin::{
    AdminUser, Backup, BackupSchedule, Backups, DayCount, PasswordReset, ServerSettings,
    ServerStats, Usage, UserUpdate, clean_settings,
};
pub use agent::describe_user_agent;
pub use ai::{
    AiAdminStatus, AiGeneralUpdate, AiInstallProgress, AiModelOption, AiPresetOption,
    AiRuntimeStatus, AiStatus, AiTestResult, AiToolSettings, AiToolState, AiToolStatus,
    AiToolUpdate, TagSuggestion, TagSuggestions, TranslateRequest, Translation,
    clean_general_update, clean_language, clean_tool_update, clean_translation, parse_suggestions,
    translate_prompt,
};
pub use ai_catalog::{AiMode, AiPreset, AiTool};
pub use autotag::{META_TAGS, META_VERSION, MediaFacts, meta_tags, meta_tags_since};
pub use edit::{Crop, EditError, EditKind, MediaEdit, Trim, edit_kind};
pub use figure::{
    COSPLAY_TAG, Figure, FigureError, FigureKind, FiguresUpdate, MAX_FIGURE_TAGS, MAX_FIGURES,
    Slot, clean_figures, cosplay_name, cosplay_tag, count_tags, is_count_tag, required_tags,
};
pub use keys::clean_api_key;
pub use library::{
    LibraryCheck, LibraryError, LibraryFailure, LibraryImport, LibraryPhase, LibraryRetry,
    LibrarySite, LibrarySource, LibraryState, MAX_ACTIVE_LIBRARY_IMPORTS, clean_credentials,
};
pub use local_ai::{Gpu, GpuKind, Hardware};
pub use media::{FORMATS, SNIFF_BYTES, detect_mime, is_media};
pub use model::*;
pub use oidc::{Identity, OidcBegin, OidcError, OidcFinish, OidcStart, SignInMethods};
pub use panels::{PanelTranslation, TextBlock, panel_prompt, parse_panel_blocks};
pub use query::{
    FigureTerm, Order, SearchQuery, SortKey, parse_query, with_color, with_exact, with_order,
};
pub use remote::{
    BooruApi, BooruSite, RemoteError, RemotePage, RemotePost, RemoteSearch, SITES, remote_tags,
};
pub use saved::clean_saved_search;
pub use share::{NewShare, Share, ShareError, SharedPost, SharedView};
pub use tag::{normalize_tag, parse_tag_list};
pub use typo::closest_tag;
pub use vocab::{SECTIONS, VARIANTS, VOCAB, VOCAB_VERSION, split_category};

/// Path prefix of the versioned JSON API.
pub const API_PREFIX: &str = "/api/v1";

/// Largest page a client may request from list endpoints.
pub const MAX_PAGE_LIMIT: u32 = 200;
/// Page size used when the client does not ask for one.
pub const DEFAULT_PAGE_LIMIT: u32 = 60;
/// Samples, the smaller copies the server makes of big images for viewing,
/// fit in this square (pixels).
pub const SAMPLE_SIZE: u32 = 2560;
