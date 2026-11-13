//! Saving a post's file or a backup.
//!
//! Webviews treat `<a download>` differently on every OS (macOS's does
//! nothing), so the desktop app writes the file into the Downloads folder
//! itself and says where. The web app and Android use the link.

use dioxus::prelude::*;

use crate::icons::{Ico, Icon};
use crate::store;

/// A file name that is safe in any folder on any OS: letters, digits,
/// `.`, `-` and `_`, not starting with a dot.
#[cfg_attr(
    not(feature = "desktop"),
    allow(dead_code, reason = "only the desktop app saves files itself")
)]
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.');
    if cleaned.is_empty() {
        "download".to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// `name`, or `name (2).ext`, `name (3).ext`, … for the first one that
/// isn't taken, so nothing is overwritten.
#[cfg_attr(
    not(feature = "desktop"),
    allow(dead_code, reason = "only the desktop app saves files itself")
)]
pub fn free_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..10_000)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| format!("{stem} ({}){ext}", chrono::Utc::now().timestamp()))
}

/// A button that saves the signed `path` (as the API returned it) under
/// `name`.
#[component]
pub fn DownloadLink(path: String, name: String, label: String) -> Element {
    native::link(path, name, label)
}

/// Saves the signed `path` under `name` without a button, for a menu or a
/// swipe. It goes on after whatever asked for it has closed.
pub fn start(path: &str, name: &str) {
    native::start(path, name);
}

#[cfg(feature = "desktop")]
mod native {
    use std::path::PathBuf;

    use super::*;
    use crate::pages::notify;

    fn folder() -> Option<PathBuf> {
        let dirs = directories::UserDirs::new()?;
        Some(
            dirs.download_dir()
                .map(PathBuf::from)
                .unwrap_or_else(|| dirs.home_dir().join("Downloads")),
        )
    }

    pub fn link(path: String, name: String, label: String) -> Element {
        let mut busy = use_signal(|| false);
        let save = move |_| {
            let (path, name) = (path.clone(), name.clone());
            spawn(async move {
                busy.set(true);
                notify(save_to_downloads(&path, &name).await);
                busy.set(false);
            });
        };
        rsx! {
            button {
                class: "icon-btn",
                aria_label: "{label}",
                title: "{label}",
                aria_busy: if busy() { "true" } else { "false" },
                disabled: busy(),
                onclick: save,
                if busy() {
                    span { class: "spinner small" }
                } else {
                    Ico { icon: Icon::Download }
                }
                span { class: "bar-label", "{label}" }
            }
        }
    }

    pub fn start(path: &str, name: &str) {
        let (path, name) = (path.to_owned(), name.to_owned());
        dioxus::core::spawn_forever(async move {
            notify(save_to_downloads(&path, &name).await);
        });
    }

    async fn save_to_downloads(path: &str, name: &str) -> String {
        let Some(client) = store::client() else {
            return t!("download-failed", reason = t!("not-signed-in"));
        };
        let Some(dir) = folder() else {
            return t!("download-failed", reason = t!("download-no-folder"));
        };
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            return t!("download-failed", reason = e.to_string());
        }
        let name = free_name(&safe_name(name), |n| dir.join(n).exists());
        let dest = dir.join(&name);
        match client.download_to(path, &dest).await {
            Ok(_) => t!("download-saved", path = dest.display().to_string()),
            Err(e) => t!("download-failed", reason = e.to_string()),
        }
    }
}

#[cfg(not(feature = "desktop"))]
mod native {
    use super::*;

    /// A link the page clicks for itself, as the browser (or Android's
    /// download manager) saves a link's file.
    pub fn start(path: &str, name: &str) {
        let Some(href) = store::client().map(|c| c.absolute(path)) else {
            return;
        };
        let eval = document::eval(
            "const [href, name] = await dioxus.recv(); \
             const a = document.createElement('a'); \
             a.href = href; a.download = name; a.rel = 'noopener'; \
             document.body.append(a); a.click(); a.remove(); return true;",
        );
        let name = name.to_owned();
        dioxus::core::spawn_forever(async move {
            if eval.send((href, name)).is_ok() {
                let _ = eval.await;
            }
        });
    }

    pub fn link(path: String, name: String, label: String) -> Element {
        let href = store::client()
            .map(|c| c.absolute(&path))
            .unwrap_or_default();
        rsx! {
            a { class: "icon-btn", href, download: "{name}", aria_label: "{label}", title: "{label}",
                Ico { icon: Icon::Download }
                span { class: "bar-label", "{label}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe() {
        assert_eq!(safe_name("ab12.png"), "ab12.png");
        assert_eq!(
            safe_name("starbooru-20260101-000000.dump"),
            "starbooru-20260101-000000.dump"
        );
        assert_eq!(safe_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_name("..\\x:y"), "_x_y");
        assert_eq!(safe_name("città.jpg"), "citt_.jpg");
        assert_eq!(safe_name(".."), "download");
        assert_eq!(safe_name(""), "download");
    }

    #[test]
    fn never_overwrites() {
        let taken = |set: &'static [&'static str]| move |n: &str| set.contains(&n);
        assert_eq!(free_name("a.png", taken(&[])), "a.png");
        assert_eq!(free_name("a.png", taken(&["a.png"])), "a (2).png");
        assert_eq!(
            free_name("a.png", taken(&["a.png", "a (2).png"])),
            "a (3).png"
        );
        assert_eq!(free_name("notes", taken(&["notes"])), "notes (2)");
        assert_eq!(free_name(".x", taken(&[".x"])), ".x (2)");
    }
}
