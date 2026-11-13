use crate::link::Link;
use chrono::Utc;
use dioxus::prelude::*;
use starbooru_core::color::Rgb;
use starbooru_core::{Identity, Role, SessionInfo, SignInMethods, Usage};

use super::api_keys::ApiKeysCard;
use super::library_import::LibraryImportCard;
use super::share::SharesCard;
use super::strength::StrengthField;
use super::{Choice, ColorPicker, ErrorBanner, Select, Spinner};
use crate::Route;
use crate::cache;
use crate::i18n::Lang;
use crate::icons::{Ico, Icon};
use crate::oidc::{self, Done};
use crate::power;
use crate::store::{self, Effects, Hand, PREFS, SESSION, Saver, SwipeAction, Theme};
use crate::system::{self, SYSTEM_HUE};
use crate::util::{human_size, time_ago};

const MIN_PASSWORD_CHARS: usize = 8;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const CREDITS: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/ATTRIBUTION.md");

/// Accent presets (OKLCH hues).
const SWATCHES: [u16; 8] = [285, 330, 20, 65, 140, 190, 245, 0];

/// Whether `hue` is the preset in use. `wallpaper` says the wallpaper's colour
/// overrides it.
fn picked(prefs: &store::Prefs, hue: u16, wallpaper: bool) -> bool {
    !wallpaper && prefs.accent_color.is_none() && prefs.accent_hue == hue
}

/// Whether the colour in use came from the picker, not a preset. A hue with no
/// colour stored came from the slider older versions had.
fn custom_in_use(prefs: &store::Prefs, wallpaper: bool) -> bool {
    !wallpaper && (prefs.accent_color.is_some() || !SWATCHES.contains(&prefs.accent_hue))
}

fn pick(hue: u16) {
    let mut prefs = PREFS.write();
    prefs.accent_hue = hue;
    prefs.accent_color = None;
    prefs.system_accent = false;
}

/// Makes `rgb` the accent. The palette is built from a hue, so a grey can't be
/// one, and this returns false for it.
fn pick_color(prefs: &mut store::Prefs, rgb: Rgb) -> bool {
    let [r, g, b] = rgb;
    let Some(hue) = system::hue_of(u32::from_be_bytes([0xFF, r, g, b])) else {
        return false;
    };
    prefs.accent_hue = hue;
    prefs.accent_color = Some(rgb);
    prefs.system_accent = false;
    true
}

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

/// Centres the accent's colour picker under its button, as far as the window
/// allows: the row of colours wraps, so the button can be anywhere across it.
const PLACE_ACCENT_MENU: &str = r#"
    const menu = document.getElementById("accent-menu");
    if (menu) {
        const button = menu.parentElement.getBoundingClientRect();
        const middle = (button.left + button.right) / 2;
        const room = document.documentElement.clientWidth - menu.offsetWidth - 8;
        const left = Math.max(8, Math.min(middle - menu.offsetWidth / 2, room));
        menu.style.left = `${left - button.left}px`;
        menu.style.transformOrigin = `${middle - left}px top`;
    }
"#;

/// The last choice of *Accent colour*: the colour picker, behind a button that
/// shows the colour when it's the one in use.
#[component]
fn AccentPicker(hue: u16, value: Rgb, on: bool) -> Element {
    let mut open = use_signal(|| false);
    let mut grey = use_signal(|| false);
    let label = t!("settings-accent-custom");
    rsx! {
        div {
            class: "color-pick",
            onkeydown: move |e: KeyboardEvent| {
                if open() && e.key() == Key::Escape {
                    e.stop_propagation();
                    open.set(false);
                }
            },
            button {
                r#type: "button",
                class: if on { "accent custom on" } else { "accent custom" },
                style: if on { "--swatch-hue: {hue}" },
                title: label.clone(),
                aria_label: label.clone(),
                aria_haspopup: "dialog",
                aria_expanded: if open() { "true" } else { "false" },
                onclick: move |_| open.toggle(),
                Ico { icon: Icon::Palette, size: 20 }
            }
            if open() {
                div { class: "select-backdrop", onclick: move |_| open.set(false) }
                div {
                    id: "accent-menu",
                    class: "color-menu",
                    role: "dialog",
                    aria_label: label.clone(),
                    onmounted: move |_| {
                        let _ = document::eval(PLACE_ACCENT_MENU);
                    },
                    ColorPicker {
                        value,
                        on_pick: move |rgb| grey.set(!pick_color(&mut PREFS.write(), rgb)),
                    }
                    if grey() {
                        p { class: "field-error", role: "alert", {t!("accent-no-hue")} }
                    }
                }
            }
        }
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
    let wallpaper = prefs.system_accent && SYSTEM_HUE().is_some();
    let accent_rgb = match prefs.accent_color {
        Some(rgb) if !wallpaper => rgb,
        _ => system::swatch_rgb(system::seed_hue(&prefs)),
    };
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
    // Hidden when the server is set never to contact other sites.
    let library_import = matches!(info.read().as_ref(), Some(Some(i)) if i.url_import);
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
                    div { class: "accents",
                        if let Some(hue) = SYSTEM_HUE() {
                            button {
                                class: if prefs.system_accent { "accent system on" } else { "accent system" },
                                style: "--swatch-hue: {hue}",
                                aria_pressed: if prefs.system_accent { "true" } else { "false" },
                                title: t!("accent-system-hint"),
                                onclick: move |_| PREFS.write().system_accent = true,
                                if prefs.system_accent { Ico { icon: Icon::Check, size: 18 } }
                                {t!("accent-system")}
                            }
                        }
                        for hue in SWATCHES {
                            button {
                                key: "{hue}",
                                class: if picked(&prefs, hue, wallpaper) { "accent on" } else { "accent" },
                                style: "--swatch-hue: {hue}",
                                aria_label: swatch_name(hue),
                                title: swatch_name(hue),
                                onclick: move |_| pick(hue),
                                if picked(&prefs, hue, wallpaper) { Ico { icon: Icon::Check, size: 18 } }
                            }
                        }
                        AccentPicker {
                            hue: prefs.accent_hue,
                            value: accent_rgb,
                            on: custom_in_use(&prefs, wallpaper),
                        }
                    }
                }
                div { class: "field",
                    span { {t!("settings-hand")} }
                    div { class: "segmented", role: "radiogroup", aria_label: t!("settings-hand"),
                        for hand in Hand::ALL {
                            button {
                                key: "{hand.as_str()}",
                                role: "radio",
                                aria_checked: if prefs.hand == hand { "true" } else { "false" },
                                class: if prefs.hand == hand { "seg on" } else { "seg" },
                                onclick: move |_| PREFS.write().hand = hand,
                                {hand.label()}
                            }
                        }
                    }
                    p { class: "muted hint", {t!("settings-hand-hint")} }
                }
                div { class: "field",
                    span { {t!("settings-saver")} }
                    div { class: "segmented", role: "radiogroup", aria_label: t!("settings-saver"),
                        for saver in Saver::ALL {
                            button {
                                key: "{saver.as_str()}",
                                role: "radio",
                                aria_checked: if prefs.battery_saver == saver { "true" } else { "false" },
                                class: if prefs.battery_saver == saver { "seg on" } else { "seg" },
                                onclick: move |_| PREFS.write().battery_saver = saver,
                                {saver.label()}
                            }
                        }
                    }
                    p { class: "muted hint", {t!("settings-saver-hint")} }
                    if prefs.battery_saver == Saver::Auto && power::device_low() {
                        p { class: "hint saver-now", {t!("settings-saver-now")} }
                    }
                }
                div { class: "field",
                    span { {t!("settings-effects")} }
                    div { class: "segmented", role: "radiogroup", aria_label: t!("settings-effects"),
                        for effects in Effects::ALL {
                            button {
                                key: "{effects.as_str()}",
                                role: "radio",
                                aria_checked: if prefs.effects == effects { "true" } else { "false" },
                                class: if prefs.effects == effects { "seg on" } else { "seg" },
                                // Choosing again, Automatic too, measures the device afresh.
                                onclick: move |_| {
                                    let mut prefs = PREFS.write();
                                    prefs.effects = effects;
                                    prefs.effects_reduced = false;
                                },
                                {effects.label()}
                            }
                        }
                    }
                    p { class: "muted hint", {t!("settings-effects-hint")} }
                    if prefs.effects == Effects::Auto && prefs.effects_reduced {
                        p { class: "hint saver-now", {t!("settings-effects-now")} }
                    }
                }
                label { class: "check",
                    input {
                        r#type: "checkbox",
                        checked: prefs.originals,
                        onchange: move |e| PREFS.write().originals = e.checked(),
                    }
                    span { {t!("settings-originals")} }
                }
                p { class: "muted hint", {t!("settings-originals-hint")} }
                label { class: "check",
                    input {
                        r#type: "checkbox",
                        checked: prefs.flush_tiles,
                        onchange: move |e| PREFS.write().flush_tiles = e.checked(),
                    }
                    span { {t!("settings-flush")} }
                }
                p { class: "muted hint", {t!("settings-flush-hint")} }
            }

            GesturesCard {}

            if cache::AVAILABLE {
                CacheCard {}
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

            if session.is_some() && store::can_upload() && library_import {
                LibraryImportCard {}
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
                SharesCard {}
                ApiKeysCard {}
            }

            AboutCard {}
        }
    }
}

/// Who made StarBooru, with what, where its code lives and which version this is.
#[component]
fn AboutCard() -> Element {
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("about-title")} }
            dl { class: "meta about-credits",
                dt { {t!("about-made-by")} } dd { "FilipNone" }
                dt { {t!("about-icons")} } dd { "MingCute, Material Symbols, Solar, Fluent UI" }
                dt { {t!("about-lettering")} } dd { "M PLUS Rounded 1c" }
                dt { {t!("about-version")} } dd { "{starbooru_core::VERSION}" }
            }
            a { class: "list-link", href: REPOSITORY, target: "_blank", rel: "noopener noreferrer",
                Ico { icon: Icon::GitHub }
                span { "GitHub" }
                span { class: "muted", {REPOSITORY.trim_start_matches("https://")} }
            }
            a { class: "list-link", href: CREDITS, target: "_blank", rel: "noopener noreferrer",
                Ico { icon: Icon::Star }
                span { {t!("about-credits")} }
                span { class: "muted", {t!("about-credits-note")} }
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

    let username = SESSION
        .read()
        .as_ref()
        .map(|s| s.user.username.clone())
        .unwrap_or_default();
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
                    StrengthField { value: new(), username,
                        input {
                            r#type: "password",
                            autocomplete: "new-password",
                            aria_invalid: too_short,
                            value: "{new}",
                            oninput: move |e| new.set(e.value()),
                        }
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

/// Size and limit of the media cache, with a button to empty it.
/// What swiping a thumbnail aside does, each way. Off until chosen, as a
/// swipe that does something by surprise can't always be taken back.
#[component]
fn GesturesCard() -> Element {
    let prefs = PREFS.read().clone();
    let options = || {
        SwipeAction::ALL
            .into_iter()
            .map(|a| Choice::new(a.as_str(), a.label()))
            .collect::<Vec<_>>()
    };
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("settings-gestures")} }
            p { class: "muted hint", {t!("settings-gestures-hint")} }
            div { class: "field",
                span { aria_hidden: "true", {t!("settings-swipe-left")} }
                Select {
                    aria_label: t!("settings-swipe-left"),
                    value: prefs.swipe_left.as_str().to_owned(),
                    options: options(),
                    onchange: move |v: String| {
                        if let Some(a) = SwipeAction::parse(&v) {
                            PREFS.write().swipe_left = a;
                        }
                    },
                }
            }
            div { class: "field",
                span { aria_hidden: "true", {t!("settings-swipe-right")} }
                Select {
                    aria_label: t!("settings-swipe-right"),
                    value: prefs.swipe_right.as_str().to_owned(),
                    options: options(),
                    onchange: move |v: String| {
                        if let Some(a) = SwipeAction::parse(&v) {
                            PREFS.write().swipe_right = a;
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn CacheCard() -> Element {
    let mut used = use_resource(|| async move { cache::size().await });
    let limit = PREFS.read().cache_mb;
    let label = |mb: u32| match mb {
        0 => t!("cache-off"),
        mb => cache::limit_label(mb),
    };
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("cache-title")} }
            p { class: "muted hint", {t!("cache-hint")} }
            div { class: "field cache-limit",
                span { aria_hidden: "true", {t!("cache-limit")} }
                Select {
                    aria_label: t!("cache-limit"),
                    value: limit.to_string(),
                    options: cache::LIMITS_MB.into_iter().map(|mb| Choice::new(mb.to_string(), label(mb))).collect::<Vec<_>>(),
                    onchange: move |v: String| {
                        if let Ok(mb) = v.parse() {
                            PREFS.write().cache_mb = mb;
                        }
                    },
                }
            }
            div { class: "row-end",
                if let Some(bytes) = used() {
                    span { class: "muted", {t!("cache-used", size = human_size(i64::try_from(bytes).unwrap_or(i64::MAX)))} }
                }
                button {
                    class: "btn tonal",
                    disabled: used() == Some(0),
                    onclick: move |_| async move {
                        cache::clear().await;
                        used.restart();
                    },
                    Ico { icon: Icon::Delete }
                    {t!("cache-clear")}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accent_keeps_the_exact_colour_it_came_from() {
        let mut prefs = store::Prefs::default();
        assert!(pick_color(&mut prefs, [0xff, 0x00, 0x00]));
        assert_eq!(prefs.accent_hue, 29);
        assert_eq!(prefs.accent_color, Some([0xff, 0x00, 0x00]));
        assert!(!prefs.system_accent);
    }

    #[test]
    fn a_grey_cant_be_the_accent() {
        let mut prefs = store::Prefs::default();
        for grey in [[0x80, 0x80, 0x80], [0, 0, 0], [0xff, 0xff, 0xff]] {
            assert!(!pick_color(&mut prefs, grey));
        }
        assert_eq!(prefs, store::Prefs::default());
    }

    #[test]
    fn one_accent_choice_shows_as_in_use() {
        let mut prefs = store::Prefs {
            accent_hue: 140,
            ..Default::default()
        };
        assert!(picked(&prefs, 140, false));
        assert!(!custom_in_use(&prefs, false));

        // An exact colour, even of a preset's hue, belongs to the picker.
        prefs.accent_color = Some([0x20, 0xb0, 0x20]);
        assert!(!picked(&prefs, 140, false));
        assert!(custom_in_use(&prefs, false));

        // A hue from the slider of older versions.
        prefs.accent_color = None;
        prefs.accent_hue = 100;
        assert!(custom_in_use(&prefs, false));

        // The wallpaper overrides both.
        assert!(!custom_in_use(&prefs, true));
        prefs.accent_hue = 140;
        assert!(!picked(&prefs, 140, true));
    }

    #[test]
    fn the_about_card_links_to_this_project() {
        assert_eq!(REPOSITORY, "https://github.com/FilipNone/StarBooru");
        // The credits link is only right while the file stays where it is.
        let attribution = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ATTRIBUTION.md");
        assert!(std::path::Path::new(attribution).exists());
    }

    #[test]
    fn accent_swatches_keep_their_own_styles() {
        // Regression: the colour search's `.swatch` rules overrode the accent's,
        // which took the tick off centre in WebKit and made the circles small.
        let css = include_str!("../../assets/main.css");
        assert_eq!(css.matches("\n.swatch {").count(), 1);
        assert_eq!(css.matches("\n.swatches {").count(), 1);
        assert_eq!(css.matches("\n.accent {").count(), 1);
    }
}
