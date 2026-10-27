//! Shared StarBooru types used by the server and every client.
//!
//! This crate does no I/O so it compiles everywhere the clients run
//! (native, `wasm32-unknown-unknown`, Android).

pub mod model;
pub mod query;
pub mod tag;

pub use model::*;
pub use query::{Order, SearchQuery, parse_query};
pub use tag::{normalize_tag, parse_tag_list};

/// Path prefix of the versioned JSON API.
pub const API_PREFIX: &str = "/api/v1";

/// Largest page a client may request from list endpoints.
pub const MAX_PAGE_LIMIT: u32 = 200;
/// Page size used when the client does not ask for one.
pub const DEFAULT_PAGE_LIMIT: u32 = 60;
