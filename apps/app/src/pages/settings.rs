use chrono::Utc;
use dioxus::prelude::*;
use starbooru_core::{Role, SessionInfo};

use super::api_keys::ApiKeysCard;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::i18n::Lang;
use crate::icons::{Ico, Icon};
use crate::store::{self, PREFS, SESSION, Theme};
use crate::util::time_ago;

const MIN_PASSWORD_CHARS: usize = 8;

/// Accent presets (OKLCH hues).
const SWATCHES: [u16; 8] = [285, 330, 20, 65, 140, 190, 245, 0];

fn swatch_name(hue: u16) -> String {
    match hue {
        285 => t!("accent-violet"),
        330 => t!("accent-pink"),
        20 => t!("accent-coral"),
        65 => t!("accent-amber"),
        140 => t!("accent-green"),
        190 => t!("accent-teal"),
        245 => t!("accent-blue"),
        _ => t!("accent-red"),
    }
}

#[component]
pub fn Settings() -> Element {
    let info = use_resource(|| async move {
        let client = store::client()?;
        client.info().await.ok()
    });
    let prefs = PREFS.read().clone();
    let session = SESSION.read().clone();
    // Bumped after a password change, which signs other sessions out.
    let mut sessions_version = use_signal(|| 0u32);

    rsx! {
        div { class: "page narrow",
            h1 { class: "page-title", {t!("nav-settings")} }

            section { class: "card",
                h2 { class: "card-title", {t!("settings-appearance")} }
                div { class: "field",
                    span { {t!("settings-theme")} }
                    div { class: "segmented", role: "radiogroup", aria_label: t!("settings-theme"),
                        for theme in Theme::ALL {
                            button {
                                key: "{theme.as_str()}",
                                role: "radio",
                                aria_checked: if prefs.theme == theme { "true" } else { "false" },
                                class: if prefs.theme == theme { "seg on" } else { "seg" },
                                onclick: move |_| PREFS.write().theme = theme,
                                {theme.label()}
                            }
                        }
                    }
                }
                div { class: "field",
                    span { {t!("settings-language")} }
                    div { class: "segmented", role: "radiogroup", aria_label: t!("settings-language"),
                        button {
                            role: "radio",
                            aria_checked: if prefs.language.is_none() { "true" } else { "false" },
                            class: if prefs.language.is_none() { "seg on" } else { "seg" },
                            onclick: move |_| PREFS.write().language = None,
                            {t!("settings-language-system")}
                        }
                        for lang in Lang::ALL {
                            button {
                                key: "{lang.code()}",
                                role: "radio",
                                lang: lang.code(),
                                aria_checked: if prefs.language == Some(lang) { "true" } else { "false" },
                                class: if prefs.language == Some(lang) { "seg on" } else { "seg" },
                                onclick: move |_| PREFS.write().language = Some(lang),
                                "{lang.name()}"
                            }
                        }
                    }
                }
                div { class: "field",
                    span { {t!("settings-accent")} }
                    div { class: "swatches",
                        for hue in SWATCHES {
                            button {
                                key: "{hue}",
                                class: if prefs.accent_hue == hue { "swatch on" } else { "swatch" },
                                style: "--swatch-hue: {hue}",
                                aria_label: swatch_name(hue),
                                title: swatch_name(hue),
                                onclick: move |_| PREFS.write().accent_hue = hue,
                                if prefs.accent_hue == hue { Ico { icon: Icon::Check, size: 18 } }
                            }
                        }
                    }
                    input {
                        r#type: "range",
                        min: "0",
                        max: "359",
                        aria_label: t!("settings-accent-custom"),
                        value: "{prefs.accent_hue}",
                        oninput: move |e| {
                            if let Ok(h) = e.value().parse::<u16>() {
                                PREFS.write().accent_hue = h.min(359);
                            }
                        },
                    }
                }
            }

            section { class: "card",
                h2 { class: "card-title", {t!("settings-library")} }
                Link { class: "list-link", to: Route::Trash {},
                    Ico { icon: Icon::Delete }
                    span { {t!("trash-title")} }
                    span { class: "muted", {t!("settings-trash-note")} }
                }
                Link { class: "list-link", to: Route::Duplicates {},
                    Ico { icon: Icon::Copy }
                    span { {t!("dup-title")} }
                    span { class: "muted", {t!("settings-dup-note")} }
                }
            }

            section { class: "card",
                h2 { class: "card-title", {t!("settings-help-legal")} }
                Link { class: "list-link", to: Route::Help {},
                    Ico { icon: Icon::Help }
                    span { {t!("help-title")} }
                    span { class: "muted", {t!("settings-help-note")} }
                }
                Link { class: "list-link", to: Route::Privacy {},
                    Ico { icon: Icon::Shield }
                    span { {t!("privacy-title")} }
                    span { class: "muted", {t!("settings-privacy-note")} }
                }
                Link { class: "list-link", to: Route::Terms {},
                    Ico { icon: Icon::Article }
                    span { {t!("terms-title")} }
                    span { class: "muted", {t!("settings-terms-note")} }
                }
            }

            if let Some(s) = session {
                section { class: "card",
                    h2 { class: "card-title", {t!("settings-account")} }
                    dl { class: "meta",
                        dt { {t!("settings-signed-in-as")} } dd { "{s.user.username}" }
                        dt { {t!("settings-role")} } dd { {role_label(s.user.role)} }
                        dt { {t!("login-server")} } dd { class: "break", "{s.server}" }
                        if let Some(Some(i)) = info.read().as_ref() {
                            dt { {t!("settings-server-version")} } dd { "{i.name} {i.version}" }
                        }
                    }
                    div { class: "row-end",
                        button { class: "btn tonal", onclick: move |_| store::sign_out(),
                            Ico { icon: Icon::Logout }
                            {t!("settings-sign-out")}
                        }
                    }
                }
                PasswordCard { onchanged: move |()| sessions_version += 1 }
                SessionsCard { version: sessions_version() }
                ApiKeysCard {}
            }
        }
    }
}

fn role_label(role: Role) -> String {
    match role {
        Role::Admin => t!("role-admin"),
        Role::User => t!("role-user"),
    }
}

#[component]
fn PasswordCard(onchanged: EventHandler<()>) -> Element {
    let mut current = use_signal(String::new);
    let mut new = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut done = use_signal(|| false);

    let too_short = !new().is_empty() && new().chars().count() < MIN_PASSWORD_CHARS;
    let mismatch = !confirm().is_empty() && confirm() != new();
    let ready = !current().is_empty()
        && new().chars().count() >= MIN_PASSWORD_CHARS
        && confirm() == new()
        && !busy();

    let submit = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let (old, fresh) = (current.peek().clone(), new.peek().clone());
        busy.set(true);
        error.set(None);
        done.set(false);
        spawn(async move {
            match client.change_password(&old, &fresh).await {
                Ok(()) => {
                    current.set(String::new());
                    new.set(String::new());
                    confirm.set(String::new());
                    done.set(true);
                    onchanged.call(());
                }
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("login-password")} }
            form { onsubmit: submit,
                label { class: "field",
                    span { {t!("password-current")} }
                    input {
                        r#type: "password",
                        autocomplete: "current-password",
                        value: "{current}",
                        oninput: move |e| current.set(e.value()),
                    }
                }
                label { class: "field",
                    span { {t!("password-new")} }
                    input {
                        r#type: "password",
                        autocomplete: "new-password",
                        aria_invalid: too_short,
                        value: "{new}",
                        oninput: move |e| new.set(e.value()),
                    }
                    if too_short {
                        small { class: "field-error", {t!("password-too-short", count = MIN_PASSWORD_CHARS)} }
                    }
                }
                label { class: "field",
                    span { {t!("password-repeat")} }
                    input {
                        r#type: "password",
                        autocomplete: "new-password",
                        aria_invalid: mismatch,
                        value: "{confirm}",
                        oninput: move |e| confirm.set(e.value()),
                    }
                    if mismatch {
                        small { class: "field-error", {t!("password-mismatch")} }
                    }
                }
                if let Some(message) = error() {
                    ErrorBanner { message }
                }
                if done() {
                    div { class: "banner info", role: "status",
                        {t!("password-changed")}
                    }
                }
                div { class: "row-end",
                    button { class: "btn", r#type: "submit", disabled: !ready,
                        if busy() { {t!("password-changing")} } else { {t!("password-change")} }
                    }
                }
            }
        }
    }
}

/// Where this account is signed in, with sign-out per device.
#[component]
fn SessionsCard(version: u32) -> Element {
    let mut list = use_signal(|| None::<Vec<SessionInfo>>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    use_effect(use_reactive!(|version| {
        let _ = version;
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.sessions().await {
                Ok(found) => list.set(Some(found)),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    }));

    let revoke = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match client.revoke_session(id).await {
                Ok(()) => {
                    if let Some(l) = list.write().as_mut() {
                        l.retain(|s| s.id != id);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let revoke_others = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match client.revoke_other_sessions().await {
                Ok(_) => {
                    if let Some(l) = list.write().as_mut() {
                        l.retain(|s| s.current);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let now = Utc::now();
    let others = list
        .read()
        .as_ref()
        .is_some_and(|l| l.iter().any(|s| !s.current));
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("sessions-title")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            match list.read().as_ref() {
                None => rsx! { Spinner {} },
                Some(sessions) => rsx! {
                    ul { class: "sessions",
                        for s in sessions.iter() {
                            li { key: "{s.id}", class: "session",
                                div { class: "session-main",
                                    strong {
                                        if s.client.is_empty() { {t!("sessions-unknown")} } else { "{s.client}" }
                                    }
                                    if s.current {
                                        span { class: "pill", {t!("sessions-this-device")} }
                                    }
                                    span { class: "muted",
                                        {t!("sessions-times", active = time_ago(s.last_used_at, now), created = time_ago(s.created_at, now))}
                                    }
                                }
                                if !s.current {
                                    SessionRevoke { id: s.id, disabled: busy(), onrevoke: revoke }
                                }
                            }
                        }
                    }
                },
            }
            if others {
                div { class: "row-end",
                    button { class: "btn tonal", disabled: busy(), onclick: revoke_others,
                        Ico { icon: Icon::Logout }
                        {t!("sessions-revoke-others")}
                    }
                }
            }
        }
    }
}

#[component]
fn SessionRevoke(id: i64, disabled: bool, onrevoke: EventHandler<i64>) -> Element {
    rsx! {
        button {
            class: "icon-btn danger",
            title: t!("sessions-revoke"),
            aria_label: t!("sessions-revoke"),
            disabled,
            onclick: move |_| onrevoke.call(id),
            Ico { icon: Icon::Logout }
        }
    }
}
