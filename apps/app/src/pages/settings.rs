use chrono::Utc;
use dioxus::prelude::*;
use starbooru_core::{Identity, Role, SessionInfo, SignInMethods, Usage};

use super::api_keys::ApiKeysCard;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::i18n::Lang;
use crate::icons::{Ico, Icon};
use crate::oidc::{self, Done};
use crate::store::{self, PREFS, SESSION, Theme};
use crate::util::{human_size, time_ago};

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
    let usage = use_resource(|| async move {
        let client = store::client()?;
        client.usage().await.ok()
    });
    let prefs = PREFS.read().clone();
    let session = SESSION.read().clone();
    // Bumped after a password change, which signs other sessions out.
    let mut sessions_version = use_signal(|| 0u32);
    let mut methods = use_signal(|| None::<SignInMethods>);
    use_effect(move || {
        let _ = sessions_version();
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            if let Ok(found) = client.sign_in_methods().await {
                methods.set(Some(found));
            }
        });
    });
    let provider = info.read().as_ref().and_then(|i| i.as_ref()?.oidc.clone());
    let has_password = methods.read().as_ref().is_none_or(|m| m.password);
    let show_methods = provider.is_some()
        || methods
            .read()
            .as_ref()
            .is_some_and(|m| !m.identities.is_empty());

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

            if session.as_ref().is_some_and(|s| s.user.role == Role::Admin) {
                section { class: "card",
                    h2 { class: "card-title", {t!("admin-server-title")} }
                    Link { class: "list-link", to: Route::Admin {},
                        Ico { icon: Icon::Shield }
                        span { {t!("admin-title")} }
                        span { class: "muted", {t!("settings-admin-note")} }
                    }
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
                        if let Some(Some(u)) = usage.read().as_ref() {
                            dt { {t!("settings-storage")} }
                            dd { class: "usage",
                                {usage_text(u)}
                                if let Some(f) = u.fraction() {
                                    span {
                                        class: if f >= 0.9 { "meter full" } else { "meter" },
                                        role: "meter",
                                        aria_valuemin: "0",
                                        aria_valuemax: "100",
                                        aria_valuenow: "{f * 100.0:.0}",
                                        aria_label: t!("settings-storage"),
                                        span { style: "width: {f * 100.0:.1}%" }
                                    }
                                }
                            }
                        }
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
                if show_methods {
                    SignInCard { methods, provider }
                }
                PasswordCard { has_password, onchanged: move |()| sessions_version += 1 }
                SessionsCard { version: sessions_version() }
                ApiKeysCard {}
            }
        }
    }
}

/// "12 posts · 1.2 GB of 5.0 GB"
fn usage_text(u: &Usage) -> String {
    let size = |b: u64| human_size(i64::try_from(b).unwrap_or(i64::MAX));
    let used = match u.quota_bytes {
        Some(q) => t!("usage-of", used = size(u.bytes), quota = size(q)),
        None => size(u.bytes),
    };
    format!("{} · {used}", t!("posts-count", count = u.posts))
}

fn role_label(role: Role) -> String {
    match role {
        Role::Admin => t!("role-admin"),
        Role::User => t!("role-user"),
    }
}

/// The account's password and provider sign-ins, with linking and
/// unlinking. The last way in can't be removed.
#[component]
fn SignInCard(methods: Signal<Option<SignInMethods>>, provider: Option<String>) -> Element {
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut waiting = use_signal(|| None::<dioxus::core::Task>);
    let link = move |_| {
        let (Some(client), Some(session)) = (store::client(), SESSION.peek().clone()) else {
            return;
        };
        busy.set(true);
        error.set(None);
        let task = spawn(async move {
            match oidc::run(client, session.server, true).await {
                Ok(None) => return,
                Ok(Some(Done::Linked(identity))) => {
                    if let Some(m) = methods.write().as_mut() {
                        m.identities.push(identity);
                    }
                }
                Ok(Some(Done::SignedIn(_))) => {}
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
            waiting.set(None);
        });
        if cfg!(not(target_arch = "wasm32")) {
            waiting.set(Some(task));
        }
    };
    let cancel = move |_| {
        if let Some(task) = waiting.take() {
            task.cancel();
        }
        busy.set(false);
    };
    let unlink = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.unlink_identity(id).await {
                Ok(()) => {
                    if let Some(m) = methods.write().as_mut() {
                        m.identities.retain(|i| i.id != id);
                    }
                }
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let now = Utc::now();
    let guard = methods.read();
    let Some(m) = guard.as_ref() else {
        return rsx! {};
    };
    let last_way_in = !m.password && m.identities.len() == 1;
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("signin-title")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            ul { class: "sessions",
                li { class: "session",
                    div { class: "session-main",
                        strong { {t!("login-password")} }
                        span { class: "muted",
                            if m.password { {t!("signin-password-set")} } else { {t!("signin-password-none")} }
                        }
                    }
                }
                for identity in m.identities.iter() {
                    li { key: "{identity.id}", class: "session",
                        div { class: "session-main",
                            strong { {identity_name(identity, provider.as_deref())} }
                            span { class: "muted",
                                {identity_times(identity, now)}
                            }
                        }
                        IdentityUnlink {
                            id: identity.id,
                            name: identity_name(identity, provider.as_deref()),
                            disabled: busy() || last_way_in,
                            onunlink: unlink,
                        }
                    }
                }
            }
            if last_way_in {
                p { class: "muted hint", {t!("signin-last")} }
            }
            if let Some(name) = provider {
                div { class: "row-end",
                    if waiting().is_some() {
                        span { class: "muted", role: "status", {t!("oidc-waiting")} }
                        button { class: "btn tonal", r#type: "button", onclick: cancel, {t!("cancel")} }
                    } else {
                        button { class: "btn tonal", r#type: "button", disabled: busy(), onclick: link,
                            Ico { icon: Icon::Link }
                            {t!("signin-link", provider = name)}
                        }
                    }
                }
            }
        }
    }
}

/// "alice@example.com", or the provider's name when it gave no label.
fn identity_name(identity: &Identity, provider: Option<&str>) -> String {
    match (&identity.label, provider) {
        (Some(label), _) if !label.is_empty() => label.clone(),
        (_, Some(provider)) => provider.to_owned(),
        _ => t!("signin-provider"),
    }
}

fn identity_times(identity: &Identity, now: chrono::DateTime<Utc>) -> String {
    let linked = time_ago(identity.created_at, now);
    match identity.last_used_at {
        Some(used) => t!("signin-times", linked = linked, used = time_ago(used, now)),
        None => t!("signin-linked", linked = linked),
    }
}

#[component]
fn IdentityUnlink(id: i64, name: String, disabled: bool, onunlink: EventHandler<i64>) -> Element {
    let mut confirm = use_signal(|| false);
    rsx! {
        button {
            class: if confirm() { "btn danger small" } else { "icon-btn danger" },
            title: t!("signin-unlink"),
            aria_label: t!("signin-unlink-named", name = name),
            disabled,
            onblur: move |_| confirm.set(false),
            onclick: move |_| {
                if confirm() {
                    onunlink.call(id);
                } else {
                    confirm.set(true);
                }
            },
            Ico { icon: Icon::Close }
            if confirm() { {t!("sure")} }
        }
    }
}

#[component]
fn PasswordCard(has_password: bool, onchanged: EventHandler<()>) -> Element {
    let mut current = use_signal(String::new);
    let mut new = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut done = use_signal(|| false);

    let too_short = !new().is_empty() && new().chars().count() < MIN_PASSWORD_CHARS;
    let mismatch = !confirm().is_empty() && confirm() != new();
    let ready = (!has_password || !current().is_empty())
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
            if !has_password {
                p { class: "muted hint", {t!("password-none")} }
            }
            form { onsubmit: submit,
                if has_password {
                    label { class: "field",
                        span { {t!("password-current")} }
                        input {
                            r#type: "password",
                            autocomplete: "current-password",
                            value: "{current}",
                            oninput: move |e| current.set(e.value()),
                        }
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
                        if busy() {
                            {t!("password-changing")}
                        } else if has_password {
                            {t!("password-change")}
                        } else {
                            {t!("password-set")}
                        }
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
