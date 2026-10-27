//! Settings › API keys: long-lived tokens for scripts and other tools.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use starbooru_core::keys::MAX_API_KEY_NAME_CHARS;
use starbooru_core::{ApiKey, CreatedApiKey, NewApiKey};

use super::{ErrorBanner, Spinner};
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{short_date, time_ago};

/// Lifetimes offered for a new key, in days; `None` never expires.
const LIFETIMES: [Option<u32>; 4] = [None, Some(30), Some(90), Some(365)];

fn lifetime_label(days: Option<u32>) -> String {
    match days {
        None => t!("keys-never-expires"),
        Some(d) => t!("keys-days", count = d),
    }
}

/// "Used 3 minutes ago · expires 27 Nov 2027", for one key.
fn key_status(key: &ApiKey, now: DateTime<Utc>) -> String {
    let used = match key.last_used_at {
        Some(t) => t!("keys-used", when = time_ago(t, now)),
        None => t!("keys-never-used"),
    };
    let expiry = match key.expires_at {
        None => t!("keys-no-expiry"),
        Some(t) if t <= now => t!("keys-expired-on", date = short_date(t)),
        Some(t) => t!("keys-expires-on", date = short_date(t)),
    };
    format!("{used} · {expiry}")
}

#[component]
pub fn ApiKeysCard() -> Element {
    let mut list = use_signal(|| None::<Vec<ApiKey>>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut read_only = use_signal(|| false);
    let mut lifetime = use_signal(|| None::<u32>);
    let mut created = use_signal(|| None::<CreatedApiKey>);
    let mut copied = use_signal(|| false);

    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.api_keys().await {
                Ok(found) => list.set(Some(found)),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });

    let create = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let request = NewApiKey {
            name: name.peek().trim().to_owned(),
            read_only: *read_only.peek(),
            expires_days: *lifetime.peek(),
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.create_api_key(&request).await {
                Ok(done) => {
                    if let Some(l) = list.write().as_mut() {
                        l.insert(0, done.key.clone());
                    }
                    name.set(String::new());
                    copied.set(false);
                    created.set(Some(done));
                }
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let remove = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.delete_api_key(id).await {
                Ok(()) => {
                    if let Some(l) = list.write().as_mut() {
                        l.retain(|k| k.id != id);
                    }
                    if created.peek().as_ref().is_some_and(|c| c.key.id == id) {
                        created.set(None);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let copy = move |_| {
        let Some(token) = created.peek().as_ref().map(|c| c.token.clone()) else {
            return;
        };
        spawn(async move {
            let eval = document::eval(
                "const token = await dioxus.recv(); \
                 await navigator.clipboard.writeText(token); \
                 return true;",
            );
            // Without clipboard access (plain http) the field stays there to
            // select and copy by hand.
            if eval.send(token).is_ok() && eval.await.is_ok() {
                copied.set(true);
            }
        });
    };

    let now = Utc::now();
    let ready = !name().trim().is_empty() && !busy();
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("keys-title")} }
            p { class: "muted hint", {t!("keys-lead")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if let Some(done) = created() {
                div { class: "banner info key-created", role: "status",
                    strong { {t!("keys-copy-now", name = done.key.name.clone())} }
                    div { class: "key-token",
                        input {
                            readonly: true,
                            spellcheck: "false",
                            aria_label: t!("keys-token"),
                            value: "{done.token}",
                            onfocus: move |_| {
                                document::eval("document.activeElement?.select?.()");
                            },
                        }
                        button { class: "btn tonal small", r#type: "button", onclick: copy,
                            Ico { icon: if copied() { Icon::Check } else { Icon::Copy }, size: 18 }
                            if copied() { {t!("keys-copied")} } else { {t!("keys-copy")} }
                        }
                    }
                }
            }
            match list.read().as_ref() {
                None => rsx! { Spinner {} },
                Some(keys) if keys.is_empty() => rsx! {
                    p { class: "muted", {t!("keys-none")} }
                },
                Some(keys) => rsx! {
                    ul { class: "sessions",
                        for k in keys.iter() {
                            li { key: "{k.id}", class: "session",
                                div { class: "session-main",
                                    strong { class: "break", "{k.name}" }
                                    code { class: "key-prefix", "{k.prefix}…" }
                                    if k.read_only {
                                        span { class: "pill", {t!("keys-read-only")} }
                                    }
                                    if k.expires_at.is_some_and(|t| t <= now) {
                                        span { class: "pill bad", {t!("keys-expired")} }
                                    }
                                    span { class: "muted", {key_status(k, now)} }
                                }
                                KeyDelete { id: k.id, name: k.name.clone(), disabled: busy(), ondelete: remove }
                            }
                        }
                    }
                },
            }
            form { class: "key-form", onsubmit: create,
                label { class: "field",
                    span { {t!("keys-name")} }
                    input {
                        placeholder: t!("keys-name-placeholder"),
                        maxlength: "{MAX_API_KEY_NAME_CHARS}",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div { class: "key-options",
                    div { class: "field",
                        span { {t!("keys-access")} }
                        div { class: "segmented", role: "radiogroup", aria_label: t!("keys-access"),
                            for (value, label) in [(false, t!("keys-full")), (true, t!("keys-read-only"))] {
                                button {
                                    key: "{value}",
                                    r#type: "button",
                                    role: "radio",
                                    aria_checked: if read_only() == value { "true" } else { "false" },
                                    class: if read_only() == value { "seg on" } else { "seg" },
                                    onclick: move |_| read_only.set(value),
                                    "{label}"
                                }
                            }
                        }
                    }
                    label { class: "field",
                        span { {t!("keys-lifetime")} }
                        select {
                            onchange: move |e| lifetime.set(e.value().parse().ok()),
                            for days in LIFETIMES {
                                option {
                                    key: "{days:?}",
                                    value: days.map(|d| d.to_string()).unwrap_or_default(),
                                    selected: lifetime() == days,
                                    {lifetime_label(days)}
                                }
                            }
                        }
                    }
                }
                div { class: "row-end",
                    button { class: "btn", r#type: "submit", disabled: !ready,
                        Ico { icon: Icon::Add }
                        {t!("keys-create")}
                    }
                }
            }
        }
    }
}

#[component]
fn KeyDelete(id: i64, name: String, disabled: bool, ondelete: EventHandler<i64>) -> Element {
    let mut confirm = use_signal(|| false);
    rsx! {
        button {
            class: if confirm() { "btn danger small" } else { "icon-btn danger" },
            title: t!("keys-delete"),
            aria_label: t!("keys-delete-named", name = name),
            disabled,
            onblur: move |_| confirm.set(false),
            onclick: move |_| {
                if confirm() {
                    ondelete.call(id);
                } else {
                    confirm.set(true);
                }
            },
            Ico { icon: Icon::Delete }
            if confirm() { {t!("sure")} }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    #[test]
    fn describes_keys() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut key = ApiKey {
            id: 1,
            name: "backup".into(),
            prefix: "sbk_12345678".into(),
            read_only: false,
            created_at: now,
            last_used_at: None,
            expires_at: None,
        };
        assert_eq!(key_status(&key, now), "Never used · Never expires");
        key.last_used_at = Some(now - TimeDelta::minutes(3));
        key.expires_at = Some(now - TimeDelta::days(1));
        assert_eq!(
            key_status(&key, now),
            "Used 3 minutes ago · Expired 14 Jan 2027"
        );
        assert_eq!(lifetime_label(Some(30)), "30 days");
        assert_eq!(lifetime_label(None), "Never");
    }
}
