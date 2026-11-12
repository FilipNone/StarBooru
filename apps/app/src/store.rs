//! App-wide state (session, preferences) and its persistence.
//!
//! Web: `localStorage`. Desktop: `state.json` in the platform config dir.
//! Android: `state.json` in the app's private files dir.

use std::sync::{Mutex, PoisonError};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use starbooru_client::Client;
use starbooru_core::color::Rgb;
use starbooru_core::{AuthResponse, User};

use crate::i18n::Lang;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub server: String,
    pub token: String,
    pub user: User,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn label(self) -> String {
        match self {
            Theme::System => t!("theme-system"),
            Theme::Light => t!("theme-light"),
            Theme::Dark => t!("theme-dark"),
        }
    }
}

/// The battery saver setting (see `power.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Saver {
    Always,
    Never,
    /// On while the battery is low or the device saves data. Also what an
    /// unknown value (from a newer version) reads as.
    #[default]
    #[serde(other)]
    Auto,
}

impl Saver {
    pub const ALL: [Saver; 3] = [Saver::Auto, Saver::Always, Saver::Never];

    pub fn as_str(self) -> &'static str {
        match self {
            Saver::Always => "always",
            Saver::Never => "never",
            Saver::Auto => "auto",
        }
    }

    pub fn label(self) -> String {
        match self {
            Saver::Always => t!("saver-always"),
            Saver::Never => t!("saver-never"),
            Saver::Auto => t!("saver-auto"),
        }
    }
}

/// The hand the one-handed controls are for: the tablet navigation rail,
/// the viewer's panel and its tools go on that side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hand {
    Left,
    Right,
    /// The usual layout: rail on the left, panel and tools on the right.
    /// Also what an unknown value (from a newer version) reads as.
    #[default]
    #[serde(other)]
    Either,
}

impl Hand {
    pub const ALL: [Hand; 3] = [Hand::Either, Hand::Left, Hand::Right];

    pub fn as_str(self) -> &'static str {
        match self {
            Hand::Either => "either",
            Hand::Left => "left",
            Hand::Right => "right",
        }
    }

    pub fn label(self) -> String {
        match self {
            Hand::Either => t!("hand-either"),
            Hand::Left => t!("hand-left"),
            Hand::Right => t!("hand-right"),
        }
    }
}

/// The shape of the gallery's tiles. Staggered tiles keep the picture's own
/// proportions, within the limits in `layout.rs`: portrait ones are at least
/// square, landscape ones at most square, and `StaggeredAll` takes both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GalleryView {
    Square,
    Landscape,
    Portrait,
    StaggeredLandscape,
    StaggeredAll,
    /// As on Pinterest. Also what an unknown value (from a newer version)
    /// reads as.
    #[default]
    #[serde(other)]
    StaggeredPortrait,
}

impl GalleryView {
    pub const FIXED: [GalleryView; 3] = [
        GalleryView::Square,
        GalleryView::Landscape,
        GalleryView::Portrait,
    ];
    pub const STAGGERED: [GalleryView; 3] = [
        GalleryView::StaggeredPortrait,
        GalleryView::StaggeredLandscape,
        GalleryView::StaggeredAll,
    ];

    pub fn is_staggered(self) -> bool {
        Self::STAGGERED.contains(&self)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GalleryView::Square => "square",
            GalleryView::Landscape => "landscape",
            GalleryView::Portrait => "portrait",
            GalleryView::StaggeredLandscape => "staggered-landscape",
            GalleryView::StaggeredAll => "staggered-all",
            GalleryView::StaggeredPortrait => "staggered-portrait",
        }
    }

    pub fn label(self) -> String {
        match self {
            GalleryView::Square => t!("view-square"),
            GalleryView::Landscape | GalleryView::StaggeredLandscape => t!("view-landscape"),
            GalleryView::Portrait | GalleryView::StaggeredPortrait => t!("view-portrait"),
            GalleryView::StaggeredAll => t!("view-everything"),
        }
    }
}

/// Another booru the user searched from StarBooru, with their account
/// there, if any. The account is kept here like the session token.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedBooru {
    /// `BooruApi::as_str`; plain text, so an API from a newer version
    /// doesn't stop the settings from loading.
    pub api: String,
    pub base: String,
    pub login: String,
    pub api_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: Theme,
    /// OKLCH hue (0–360) the whole palette is derived from.
    pub accent_hue: u16,
    /// The exact colour `accent_hue` was taken from, to show it again in the
    /// picker; `None` for a preset.
    pub accent_color: Option<Rgb>,
    /// Follow the wallpaper's colour where the system has one (Android 12+)
    /// instead of `accent_hue`.
    pub system_accent: bool,
    pub last_server: String,
    /// `None` follows the system language.
    pub language: Option<Lang>,
    /// Show originals in the viewer even when the server has a sample.
    pub originals: bool,
    /// Media cache limit in MiB on desktop and Android; 0 is off.
    pub cache_mb: u32,
    pub hand: Hand,
    pub battery_saver: Saver,
    /// Other boorus searched, the latest first.
    pub boorus: Vec<SavedBooru>,
    pub gallery_view: GalleryView,
    /// Columns in the gallery; `None` aims for a set tile width.
    pub gallery_columns: Option<u8>,
    /// No space between thumbnails and no rounded corners.
    pub flush_tiles: bool,
    /// The editor keeps the original post; otherwise it goes to the recycle
    /// bin once the edited copy is saved.
    pub keep_original: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent_hue: 285,
            accent_color: None,
            system_accent: true,
            last_server: String::new(),
            language: None,
            originals: false,
            cache_mb: crate::cache::DEFAULT_LIMIT_MB,
            hand: Hand::Either,
            battery_saver: Saver::Auto,
            boorus: Vec::new(),
            gallery_view: GalleryView::default(),
            gallery_columns: None,
            flush_tiles: false,
            keep_original: false,
        }
    }
}

/// Where the gallery was for one search. It remembers the tile at the top of
/// the screen and how far down it was, as a scroll position alone means
/// nothing once new posts arrive or the layout changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spot {
    pub q: String,
    pub post: Option<i64>,
    /// The tile's distance from the top of the screen in pixels.
    pub offset: f64,
    /// The scroll position, for when that tile has gone.
    pub y: f64,
    /// How many posts were loaded.
    pub count: u32,
}

const MAX_SPOTS: usize = 30;

/// The searches the gallery has shown this account, the latest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GalleryMemory {
    owner: String,
    spots: Vec<Spot>,
}

impl GalleryMemory {
    fn visit(&mut self, q: &str) {
        let spot = self
            .spots
            .iter()
            .position(|s| s.q == q)
            .map(|i| self.spots.remove(i))
            .unwrap_or_else(|| Spot {
                q: q.to_owned(),
                ..Spot::default()
            });
        self.spots.insert(0, spot);
        self.spots.truncate(MAX_SPOTS);
    }

    fn remember(&mut self, spot: Spot) {
        self.visit(&spot.q);
        if let Some(first) = self.spots.first_mut() {
            *first = spot;
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    session: Option<Session>,
    prefs: Prefs,
    memory: GalleryMemory,
}

fn load() -> Stored {
    backend::read()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub static SESSION: GlobalSignal<Option<Session>> = Signal::global(|| load().session);
pub static PREFS: GlobalSignal<Prefs> = Signal::global(|| load().prefs);
static MEMORY: GlobalSignal<GalleryMemory> = Signal::global(|| load().memory);

/// Writes the current state. Called from an effect whenever it changes.
pub fn persist() {
    let stored = Stored {
        session: SESSION.peek().clone(),
        prefs: PREFS.peek().clone(),
        memory: MEMORY.peek().clone(),
    };
    if let Ok(json) = serde_json::to_string(&stored) {
        backend::write(&json);
    }
}

/// Whether this account can add posts. Every signed-in account can today. The
/// Upload tab and dropping files on the gallery both ask here.
pub fn can_upload() -> bool {
    SESSION.read().is_some()
}

/// Who the app is signed in as, so one account never sees what another
/// searched.
pub fn owner() -> String {
    SESSION
        .peek()
        .as_ref()
        .map_or_else(String::new, |s| format!("{}|{}", s.server, s.user.id))
}

fn update_memory(change: impl FnOnce(&mut GalleryMemory)) {
    let owner = owner();
    let mut memory = MEMORY.write();
    if memory.owner != owner {
        *memory = GalleryMemory {
            owner,
            spots: Vec::new(),
        };
    }
    change(&mut memory);
    drop(memory);
    persist();
}

/// Makes `q` the search the gallery goes back to.
pub fn visit_search(q: &str) {
    update_memory(|memory| memory.visit(q));
}

pub fn remember_spot(spot: Spot) {
    update_memory(|memory| memory.remember(spot));
}

/// Where the gallery was for `q`.
pub fn spot(q: &str) -> Option<Spot> {
    let memory = MEMORY.peek();
    (memory.owner == owner())
        .then(|| memory.spots.iter().find(|s| s.q == q).cloned())
        .flatten()
}

/// The search the gallery showed last.
pub fn last_search() -> String {
    let memory = MEMORY.peek();
    (memory.owner == owner())
        .then(|| memory.spots.first().map(|s| s.q.clone()))
        .flatten()
        .unwrap_or_default()
}

// Reuse one HTTP client (and its connection pool) per session, on any thread.
static CLIENT: Mutex<Option<(String, String, Client)>> = Mutex::new(None);

/// API client for the signed-in session, if any.
pub fn client() -> Option<Client> {
    let session = SESSION.peek();
    let session = session.as_ref()?;
    let mut cache = CLIENT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((server, token, client)) = cache.as_ref()
        && *server == session.server
        && *token == session.token
    {
        return Some(client.clone());
    }
    let client = Client::new(&session.server)
        .ok()?
        .with_token(Some(session.token.clone()));
    *cache = Some((
        session.server.clone(),
        session.token.clone(),
        client.clone(),
    ));
    Some(client)
}

pub fn sign_in(server: String, auth: AuthResponse) {
    PREFS.write().last_server = server.clone();
    *SESSION.write() = Some(Session {
        server,
        token: auth.token,
        user: auth.user,
    });
}

/// Ends the session on the server (best effort) and locally.
pub fn sign_out() {
    if let Some(client) = client() {
        spawn(async move {
            let _ = client.logout().await;
        });
    }
    *MEMORY.write() = GalleryMemory::default();
    forget_session();
}

/// Drops the local session, e.g. after the server answered 401.
pub fn forget_session() {
    *SESSION.write() = None;
}

/// Server URL to prefill on the sign-in screen.
pub fn default_server() -> String {
    backend::default_server()
}

#[cfg(target_arch = "wasm32")]
mod backend {
    const KEY: &str = "starbooru";

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn read() -> Option<String> {
        storage()?.get_item(KEY).ok()?
    }

    pub fn write(json: &str) {
        if let Some(storage) = storage() {
            let _ = storage.set_item(KEY, json);
        }
    }

    /// The web app is normally served by the instance itself. Dev builds can
    /// bake in another server with `STARBOORU_SERVER` at compile time.
    pub fn default_server() -> String {
        if let Some(server) = option_env!("STARBOORU_SERVER").filter(|s| !s.is_empty()) {
            return server.to_owned();
        }
        web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        #[cfg(target_os = "android")]
        let dir = crate::system::android::files_dir();
        #[cfg(not(target_os = "android"))]
        let dir = directories::ProjectDirs::from("eu", "filipnone", "StarBooru")
            .map(|d| d.config_dir().to_path_buf());
        dir.map(|d| d.join("state.json"))
    }

    pub fn read() -> Option<String> {
        std::fs::read_to_string(path()?).ok()
    }

    pub fn write(json: &str) {
        let Some(path) = path() else { return };
        let Some(dir) = path.parent() else { return };
        let tmp = path.with_extension("json.tmp");
        let result = std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(&tmp, json))
            .and_then(|()| std::fs::rename(&tmp, &path));
        if let Err(e) = result {
            eprintln!("starbooru: cannot save state to {}: {e}", path.display());
        }
    }

    /// `STARBOORU_SERVER` at run time, else at compile time (Android dev builds).
    pub fn default_server() -> String {
        std::env::var("STARBOORU_SERVER")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| option_env!("STARBOORU_SERVER").map(str::to_owned))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_prefs_still_load() {
        let prefs: Prefs = serde_json::from_str(r#"{"theme":"dark","accent_hue":120}"#).unwrap();
        assert_eq!(prefs.theme, Theme::Dark);
        assert_eq!(prefs.accent_color, None);
        assert_eq!(prefs.hand, Hand::Either);
        assert_eq!(prefs.battery_saver, Saver::Auto);
        assert!(prefs.boorus.is_empty());
        let prefs: Prefs = serde_json::from_str(r#"{"hand":"left"}"#).unwrap();
        assert_eq!(prefs.hand, Hand::Left);
        // An unknown hand from a newer version falls back rather than failing.
        let prefs: Prefs = serde_json::from_str(r#"{"hand":"both","theme":"light"}"#).unwrap();
        assert_eq!((prefs.hand, prefs.theme), (Hand::Either, Theme::Light));
    }

    #[test]
    fn the_latest_search_comes_first_and_keeps_its_spot() {
        let mut memory = GalleryMemory::default();
        memory.remember(Spot {
            q: "cat".into(),
            post: Some(7),
            offset: 12.0,
            y: 900.0,
            count: 120,
        });
        memory.visit("dog");
        assert_eq!(memory.spots.first().map(|s| s.q.as_str()), Some("dog"));
        assert_eq!(memory.spots.first().and_then(|s| s.post), None);
        memory.visit("cat");
        assert_eq!(memory.spots.first().map(|s| s.q.as_str()), Some("cat"));
        assert_eq!(memory.spots.first().and_then(|s| s.post), Some(7));
        assert_eq!(memory.spots.len(), 2);
    }

    #[test]
    fn old_searches_drop_off() {
        let mut memory = GalleryMemory::default();
        for i in 0..MAX_SPOTS + 5 {
            memory.visit(&format!("tag{i}"));
        }
        assert_eq!(memory.spots.len(), MAX_SPOTS);
        assert!(memory.spots.iter().all(|s| s.q != "tag0"));
    }

    #[test]
    fn stored_state_without_memory_still_loads() {
        let stored: Stored = serde_json::from_str(r#"{"prefs":{"theme":"dark"}}"#).unwrap();
        assert_eq!(stored.memory, GalleryMemory::default());
    }

    #[test]
    fn gallery_prefs_default_and_round_trip() {
        let prefs: Prefs = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(prefs.gallery_view, GalleryView::StaggeredPortrait);
        assert_eq!(prefs.gallery_columns, None);
        assert!(!prefs.flush_tiles);
        for view in GalleryView::FIXED.into_iter().chain(GalleryView::STAGGERED) {
            let json = format!(r#"{{"gallery_view":"{}"}}"#, view.as_str());
            let prefs: Prefs = serde_json::from_str(&json).unwrap();
            assert_eq!(prefs.gallery_view, view);
        }
        // An unknown view from a newer version falls back rather than failing.
        let prefs: Prefs =
            serde_json::from_str(r#"{"gallery_view":"hexagons","gallery_columns":4}"#).unwrap();
        assert_eq!(prefs.gallery_view, GalleryView::StaggeredPortrait);
        assert_eq!(prefs.gallery_columns, Some(4));
    }
}
