//! App-wide state (session, preferences) and its persistence.
//!
//! Web: `localStorage`. Desktop/Android: `state.json` in the platform config dir.

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: Theme,
    /// OKLCH hue (0–360) the whole palette is derived from.
    pub accent_hue: u16,
    pub last_server: String,
    /// `None` follows the system language.
    pub language: Option<Lang>,
    /// Show originals in the viewer even when the server has a sample.
    pub originals: bool,
    /// Media cache limit in MiB on desktop and Android; 0 is off.
    pub cache_mb: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent_hue: 285,
            last_server: String::new(),
            language: None,
            originals: false,
            cache_mb: crate::cache::DEFAULT_LIMIT_MB,
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
        directories::ProjectDirs::from("eu", "filipnone", "StarBooru")
            .map(|d| d.config_dir().join("state.json"))
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
