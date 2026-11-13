//! Administration (admins only): server stats, accounts with their usage
//! and storage limits, database backups and the server settings.

use chrono::Utc;
use dioxus::prelude::*;
use starbooru_core::admin::MAX_INSTANCE_NAME_CHARS;
use starbooru_core::{AdminUser, Role, ServerSettings, ServerStats, UserUpdate};

use super::admin_ai::AiCard;
use super::admin_backups::BackupsTab;
use super::{ErrorBanner, Spinner};
use crate::icons::{Ico, Icon};
use crate::store::{self, SESSION};
use crate::util::{human_size, short_date, time_ago};

const MIN_PASSWORD_CHARS: usize = 8;
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
/// Largest storage limit the field takes, in GB.
const MAX_QUOTA_GB: f64 = 1_000_000.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Accounts,
    Backups,
    Server,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Overview, Tab::Accounts, Tab::Backups, Tab::Server];

    fn label(self) -> String {
        match self {
            Tab::Overview => t!("admin-tab-overview"),
            Tab::Accounts => t!("admin-tab-accounts"),
            Tab::Backups => t!("admin-tab-backups"),
            Tab::Server => t!("admin-tab-server"),
        }
    }
}

/// A storage limit typed in GB: empty is no limit. Takes `,` or `.` as the
/// decimal mark.
fn parse_quota(text: &str) -> Result<Option<u64>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let gb: f64 = text.replace(',', ".").parse().map_err(|_| ())?;
    if !gb.is_finite() || !(0.0..=MAX_QUOTA_GB).contains(&gb) {
        return Err(());
    }
    Ok(Some((gb * GIB).round() as u64))
}

/// A limit in GB for the field: `1.5`, `10`, `0.25`.
fn quota_text(bytes: u64) -> String {
    let text = format!("{:.2}", bytes as f64 / GIB);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// "3 posts · 1.2 GB of 5.0 GB"
fn usage_line(user: &AdminUser) -> String {
    let size = human_size(i64::try_from(user.bytes).unwrap_or(i64::MAX));
    let used = match user.quota_bytes {
        Some(q) => t!(
            "usage-of",
            used = size,
            quota = human_size(i64::try_from(q).unwrap_or(i64::MAX))
        ),
        None => size,
    };
    format!("{} · {used}", t!("posts-count", count = user.posts))
}

#[component]
pub fn Admin() -> Element {
    let admin = SESSION
        .read()
        .as_ref()
        .is_some_and(|s| s.user.role == Role::Admin);
    let mut tab = use_signal(|| Tab::Overview);
    // What was changed here shows in the viewer and the upload page at once.
    use_drop(store::forget_server_answers);
    rsx! {
        div { class: "page narrow",
            h1 { class: "page-title", {t!("admin-title")} }
            if !admin {
                div { class: "card empty",
                    p { class: "muted", {t!("admin-only")} }
                }
            } else {
                div { class: "segmented admin-tabs", role: "tablist", aria_label: t!("admin-title"),
                    for t in Tab::ALL {
                        button {
                            key: "{t.label()}",
                            role: "tab",
                            aria_selected: if tab() == t { "true" } else { "false" },
                            class: if tab() == t { "seg on" } else { "seg" },
                            onclick: move |_| tab.set(t),
                            {t.label()}
                        }
                    }
                }
                match tab() {
                    Tab::Overview => rsx! { Overview {} },
                    Tab::Accounts => rsx! { Accounts {} },
                    Tab::Backups => rsx! { BackupsTab {} },
                    Tab::Server => rsx! {
                        ServerCard {}
                        AiCard {}
                    },
                }
            }
        }
    }
}

#[component]
fn Overview() -> Element {
    let mut stats = use_signal(|| None::<ServerStats>);
    let mut error = use_signal(|| None::<String>);
    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.admin_stats().await {
                Ok(s) => stats.set(Some(s)),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });

    let now = Utc::now();
    let size = |b: u64| human_size(i64::try_from(b).unwrap_or(i64::MAX));
    let on_off = |on: bool| if on { t!("admin-on") } else { t!("admin-off") };
    rsx! {
        if let Some(message) = error() {
            ErrorBanner { message }
        }
        match stats.read().as_ref() {
            None if error.read().is_none() => rsx! { Spinner {} },
            None => rsx! {},
            Some(s) => {
                let tiles = [
                    (t!("admin-posts"), s.posts().to_string()),
                    (t!("admin-images"), s.images.to_string()),
                    (t!("admin-animations"), s.animations.to_string()),
                    (t!("admin-videos"), s.videos.to_string()),
                    (t!("admin-bin"), s.bin_posts.to_string()),
                    (t!("admin-tags"), s.tags.to_string()),
                    (t!("admin-accounts"), s.users.to_string()),
                    (t!("admin-media"), size(s.media_bytes)),
                ];
                let most = s.uploads_by_day.iter().map(|d| d.posts).max().unwrap_or(0).max(1);
                let total: u64 = s.uploads_by_day.iter().map(|d| d.posts).sum();
                let first = s.uploads_by_day.first().map(|d| d.day.format("%d/%m").to_string());
                let last = s.uploads_by_day.last().map(|d| d.day.format("%d/%m").to_string());
                rsx! {
                    div { class: "stat-grid",
                        for (label, value) in tiles {
                            div { key: "{label}", class: "stat",
                                span { class: "stat-value", "{value}" }
                                span { class: "stat-label", "{label}" }
                            }
                        }
                    }
                    section { class: "card",
                        h2 { class: "card-title", {t!("admin-uploads-title")} }
                        p { class: "muted hint", {t!("admin-uploads-total", count = total)} }
                        div {
                            class: "bars",
                            role: "img",
                            aria_label: t!("admin-uploads-total", count = total),
                            for d in s.uploads_by_day.iter() {
                                div {
                                    key: "{d.day}",
                                    class: if d.posts == 0 { "bar empty" } else { "bar" },
                                    style: "height: {d.posts as f64 / most as f64 * 100.0:.1}%",
                                    title: "{d.day.format(\"%d/%m\")}: {d.posts}",
                                }
                            }
                        }
                        div { class: "bars-axis muted",
                            span { {first} }
                            span { {last} }
                        }
                    }
                    section { class: "card",
                        h2 { class: "card-title", {t!("admin-server-title")} }
                        dl { class: "meta",
                            dt { {t!("settings-server-version")} } dd { "{s.version}" }
                            dt { {t!("admin-running")} } dd { {time_ago(s.started_at, now)} }
                            dt { {t!("admin-upload-limit")} } dd { {size(s.max_upload_bytes)} }
                            dt { {t!("admin-database")} } dd { {size(s.database_bytes)} }
                            dt { {t!("admin-bin-size")} } dd { {size(s.bin_bytes)} }
                            dt { {t!("admin-disabled-accounts")} } dd { "{s.disabled_users}" }
                            dt { {t!("admin-video-thumbs")} } dd { {on_off(s.video_thumbnails)} }
                            dt { {t!("admin-url-import")} } dd { {on_off(s.url_import)} }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Accounts() -> Element {
    let mut list = use_signal(|| None::<Vec<AdminUser>>);
    let mut error = use_signal(|| None::<String>);
    let mut open = use_signal(|| None::<i64>);
    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.admin_users().await {
                Ok(found) => list.set(Some(found)),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });
    let me = SESSION.read().as_ref().map(|s| s.user.id);
    let now = Utc::now();
    let changed = move |user: AdminUser| {
        if let Some(l) = list.write().as_mut()
            && let Some(slot) = l.iter_mut().find(|u| u.id == user.id)
        {
            *slot = user;
        }
    };
    let removed = move |id: i64| {
        if let Some(l) = list.write().as_mut() {
            l.retain(|u| u.id != id);
        }
        open.set(None);
    };
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("admin-accounts")} }
            p { class: "muted hint", {t!("admin-accounts-lead")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            match list.read().as_ref() {
                None if error.read().is_none() => rsx! { Spinner {} },
                None => rsx! {},
                Some(users) => rsx! {
                    ul { class: "sessions",
                        for u in users.iter() {
                            li { key: "{u.id}", class: "session account",
                                div { class: "session-main",
                                    strong { class: "break", "{u.username}" }
                                    if u.role == Role::Admin {
                                        span { class: "pill", {t!("role-admin")} }
                                    }
                                    if u.disabled {
                                        span { class: "pill bad", {t!("admin-disabled")} }
                                    }
                                    if me == Some(u.id) {
                                        span { class: "pill", {t!("admin-you")} }
                                    }
                                    span { class: "muted",
                                        {usage_line(u)}
                                        " · "
                                        match u.last_active {
                                            Some(t) => t!("admin-active", when = time_ago(t, now)),
                                            None => t!("admin-never-active"),
                                        }
                                        " · "
                                        {t!("admin-joined", date = short_date(u.created_at))}
                                    }
                                }
                                if me != Some(u.id) {
                                    {
                                        let id = u.id;
                                        let expanded = open() == Some(id);
                                        rsx! {
                                            button {
                                                class: if expanded { "icon-btn on" } else { "icon-btn" },
                                                title: t!("admin-manage"),
                                                aria_label: t!("admin-manage-named", name = u.username.clone()),
                                                aria_expanded: if expanded { "true" } else { "false" },
                                                onclick: move |_| open.set(if expanded { None } else { Some(id) }),
                                                Ico { icon: Icon::Tune }
                                            }
                                            if expanded {
                                                AccountTools { user: u.clone(), onchange: changed, onremove: removed }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// Role, storage limit, password reset, disable and delete for one account.
#[component]
fn AccountTools(
    user: AdminUser,
    onchange: EventHandler<AdminUser>,
    onremove: EventHandler<i64>,
) -> Element {
    let id = user.id;
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut done = use_signal(|| None::<String>);
    let mut quota = use_signal(|| user.quota_bytes.map(quota_text).unwrap_or_default());
    let mut password = use_signal(String::new);
    let mut confirm_delete = use_signal(|| false);

    let mut update = move |change: UserUpdate, message: String| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        done.set(None);
        spawn(async move {
            match client.update_user(id, &change).await {
                Ok(u) => {
                    onchange.call(u);
                    done.set(Some(message));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let save_quota = move |e: FormEvent| {
        e.prevent_default();
        match parse_quota(&quota.peek()) {
            Ok(q) => update(
                UserUpdate {
                    quota_bytes: Some(q),
                    ..UserUpdate::default()
                },
                t!("admin-limit-saved"),
            ),
            Err(()) => error.set(Some(t!("admin-limit-invalid"))),
        }
    };
    let reset = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let new = password.peek().clone();
        busy.set(true);
        error.set(None);
        done.set(None);
        spawn(async move {
            match client.reset_user_password(id, &new).await {
                Ok(()) => {
                    password.set(String::new());
                    done.set(Some(t!("admin-password-reset-done")));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let remove = move |_| {
        if !confirm_delete() {
            confirm_delete.set(true);
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.delete_user(id).await {
                Ok(()) => onremove.call(id),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let quota_ok = parse_quota(&quota()).is_ok();
    let password_ok = password().chars().count() >= MIN_PASSWORD_CHARS;
    let disabled = user.disabled;
    rsx! {
        div { class: "account-tools",
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if let Some(message) = done() {
                div { class: "banner info", role: "status", "{message}" }
            }
            div { class: "field",
                span { {t!("settings-role")} }
                div { class: "segmented", role: "radiogroup", aria_label: t!("settings-role"),
                    for role in [Role::User, Role::Admin] {
                        button {
                            key: "{role.as_str()}",
                            r#type: "button",
                            role: "radio",
                            disabled: busy(),
                            aria_checked: if user.role == role { "true" } else { "false" },
                            class: if user.role == role { "seg on" } else { "seg" },
                            onclick: move |_| {
                                if user.role != role {
                                    update(
                                        UserUpdate { role: Some(role), ..UserUpdate::default() },
                                        t!("admin-role-saved"),
                                    );
                                }
                            },
                            if role == Role::Admin { {t!("role-admin")} } else { {t!("role-user")} }
                        }
                    }
                }
            }
            form { class: "field", onsubmit: save_quota,
                label { r#for: "quota-{id}", {t!("admin-limit")} }
                div { class: "inline-input",
                    input {
                        id: "quota-{id}",
                        inputmode: "decimal",
                        placeholder: t!("admin-limit-none"),
                        aria_invalid: if quota_ok { "false" } else { "true" },
                        value: "{quota}",
                        oninput: move |e| quota.set(e.value()),
                    }
                    button { class: "btn tonal", r#type: "submit", disabled: busy() || !quota_ok,
                        {t!("admin-limit-save")}
                    }
                }
                span { class: "muted field-hint", {t!("admin-limit-hint")} }
            }
            form { class: "field", onsubmit: reset,
                label { r#for: "password-{id}", {t!("admin-password-new")} }
                div { class: "inline-input",
                    input {
                        id: "password-{id}",
                        r#type: "password",
                        autocomplete: "new-password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                    }
                    button { class: "btn tonal", r#type: "submit", disabled: busy() || !password_ok,
                        {t!("admin-password-reset")}
                    }
                }
                span { class: "muted field-hint", {t!("admin-password-hint")} }
            }
            div { class: "row-end",
                button {
                    class: "btn tonal small",
                    r#type: "button",
                    disabled: busy(),
                    onclick: move |_| {
                        let message = if disabled { t!("admin-enabled-done") } else { t!("admin-disabled-done") };
                        update(UserUpdate { disabled: Some(!disabled), ..UserUpdate::default() }, message);
                    },
                    if disabled { {t!("admin-enable")} } else { {t!("admin-disable")} }
                }
                button {
                    class: "btn danger small",
                    r#type: "button",
                    disabled: busy(),
                    onblur: move |_| confirm_delete.set(false),
                    onclick: remove,
                    Ico { icon: Icon::Delete }
                    if confirm_delete() { {t!("admin-delete-confirm")} } else { {t!("admin-delete")} }
                }
            }
            p { class: "muted field-hint", {t!("admin-delete-note")} }
        }
    }
}

#[component]
fn ServerCard() -> Element {
    let mut name = use_signal(String::new);
    let mut open = use_signal(|| false);
    let mut days = use_signal(String::new);
    let mut meta_tags = use_signal(|| true);
    let mut loaded = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut saved = use_signal(|| false);
    let mut fill = move |s: ServerSettings| {
        name.set(s.name);
        open.set(s.allow_registration);
        days.set(s.trash_days.to_string());
        meta_tags.set(s.auto_meta_tags);
        loaded.set(true);
    };
    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.admin_settings().await {
                Ok(s) => fill(s),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });
    let parsed_days = days().trim().parse::<u32>().ok();
    let save = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let Some(trash_days) = days.peek().trim().parse::<u32>().ok() else {
            return;
        };
        let settings = ServerSettings {
            name: name.peek().clone(),
            allow_registration: *open.peek(),
            trash_days,
            auto_meta_tags: *meta_tags.peek(),
        };
        busy.set(true);
        error.set(None);
        saved.set(false);
        spawn(async move {
            match client.set_admin_settings(&settings).await {
                Ok(s) => {
                    fill(s);
                    saved.set(true);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("admin-tab-server")} }
            p { class: "muted hint", {t!("admin-server-lead")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if !loaded() {
                if error.read().is_none() {
                    Spinner {}
                }
            } else {
                form { onsubmit: save, oninput: move |_| saved.set(false),
                    label { class: "field",
                        span { {t!("admin-name")} }
                        input {
                            maxlength: "{MAX_INSTANCE_NAME_CHARS}",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                        span { class: "muted field-hint", {t!("admin-name-hint")} }
                    }
                    div { class: "field",
                        label { class: "check",
                            input {
                                r#type: "checkbox",
                                checked: open(),
                                onchange: move |e| open.set(e.checked()),
                            }
                            span { {t!("admin-registration")} }
                        }
                        span { class: "muted field-hint", {t!("admin-registration-hint")} }
                    }
                    label { class: "field",
                        span { {t!("admin-trash-days")} }
                        input {
                            r#type: "number",
                            inputmode: "numeric",
                            min: "0",
                            max: "3650",
                            aria_invalid: if parsed_days.is_some() { "false" } else { "true" },
                            value: "{days}",
                            oninput: move |e| days.set(e.value()),
                        }
                        span { class: "muted field-hint", {t!("admin-trash-days-hint")} }
                    }
                    div { class: "field",
                        label { class: "check",
                            input {
                                r#type: "checkbox",
                                checked: meta_tags(),
                                onchange: move |e| meta_tags.set(e.checked()),
                            }
                            span { {t!("admin-auto-tags")} }
                        }
                        span { class: "muted field-hint", {t!("admin-auto-tags-hint")} }
                    }
                    div { class: "row-end",
                        if saved() {
                            span { class: "muted saved-note", role: "status",
                                Ico { icon: Icon::Check, size: 18 }
                                {t!("admin-saved")}
                            }
                        }
                        button {
                            class: "btn",
                            r#type: "submit",
                            disabled: busy() || parsed_days.is_none() || name().trim().is_empty(),
                            {t!("admin-save")}
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_field() {
        assert_eq!(parse_quota(""), Ok(None));
        assert_eq!(parse_quota("  "), Ok(None));
        assert_eq!(parse_quota("1"), Ok(Some(1 << 30)));
        assert_eq!(parse_quota("0,5"), Ok(Some(1 << 29)));
        assert_eq!(parse_quota("0"), Ok(Some(0)));
        assert_eq!(parse_quota("-1"), Err(()));
        assert_eq!(parse_quota("lots"), Err(()));
        assert_eq!(parse_quota("NaN"), Err(()));
        assert_eq!(parse_quota("1e300"), Err(()));
        assert_eq!(quota_text(1 << 30), "1");
        assert_eq!(quota_text(3 << 29), "1.5");
        assert_eq!(quota_text(10 << 30), "10");
        assert_eq!(quota_text(1 << 28), "0.25");
        for text in ["2.5", "0.25", "100"] {
            let bytes = parse_quota(text).unwrap().unwrap();
            assert_eq!(quota_text(bytes), text);
        }
    }

    #[test]
    fn describes_usage() {
        let mut user = AdminUser {
            id: 2,
            username: "someone".into(),
            role: Role::User,
            created_at: Utc::now(),
            disabled: false,
            posts: 3,
            bytes: 1536,
            quota_bytes: None,
            last_active: None,
        };
        assert_eq!(usage_line(&user), "3 posts · 1.5 KB");
        user.quota_bytes = Some(1 << 30);
        assert_eq!(usage_line(&user), "3 posts · 1.5 KB of 1.0 GB");
    }
}
