//! App-wide state (session, preferences) and its persistence.
//!
//! Web: `localStorage`. Desktop: `state.json` in the platform config dir.
//! Android: `state.json` in the app's private files dir.

use std::cell::RefCell;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use starbooru_client::Client;
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
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent_hue: 285,
            system_accent: true,
            last_server: String::new(),
            language: None,
            originals: false,
            cache_mb: crate::cache::DEFAULT_LIMIT_MB,
            hand: Hand::Either,
            battery_saver: Saver::Auto,
            boorus: Vec::new(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    session: Option<Session>,
    prefs: Prefs,
}

fn load() -> Stored {
    backend::read()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub static SESSION: GlobalSignal<Option<Session>> = Signal::global(|| load().session);
pub static PREFS: GlobalSignal<Prefs> = Signal::global(|| load().prefs);

/// Writes the current state. Called from an effect whenever it changes.
pub fn persist() {
    let stored = Stored {
        session: SESSION.peek().clone(),
        prefs: PREFS.peek().clone(),
    };
    if let Ok(json) = serde_json::to_string(&stored) {
        backend::write(&json);
    }
}

thread_local! {
    // Reuse one HTTP client (and its connection pool) per session.
    #[cfg_attr(
        target_os = "android",
        allow(clippy::missing_const_for_thread_local, reason = "false positive: it is const")
    )]
    static CLIENT: RefCell<Option<(String, String, Client)>> = const { RefCell::new(None) };
}

/// API client for the signed-in session, if any.
pub fn client() -> Option<Client> {
    let session = SESSION.peek();
    let session = session.as_ref()?;
    CLIENT.with_borrow_mut(|cache| {
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
    })
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
        assert_eq!(prefs.hand, Hand::Either);
        assert_eq!(prefs.battery_saver, Saver::Auto);
        assert!(prefs.boorus.is_empty());
        let prefs: Prefs = serde_json::from_str(r#"{"hand":"left"}"#).unwrap();
        assert_eq!(prefs.hand, Hand::Left);
        // An unknown hand from a newer version falls back rather than failing.
        let prefs: Prefs = serde_json::from_str(r#"{"hand":"both","theme":"light"}"#).unwrap();
        assert_eq!((prefs.hand, prefs.theme), (Hand::Either, Theme::Light));
    }
}
