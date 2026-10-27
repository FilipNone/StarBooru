//! Administration › Backups: database backups made by hand or on a
//! schedule, with download links.

use dioxus::prelude::*;
use starbooru_core::admin::{MAX_BACKUP_DAYS, MAX_BACKUP_KEEP};
use starbooru_core::{Backup, BackupSchedule, Backups};

use super::{ErrorBanner, Spinner};
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{human_size, short_date};

/// The command shown under "Restoring".
const RESTORE_COMMAND: &str =
    "pg_restore --clean --if-exists --no-owner -d \"$DATABASE_URL\" starbooru-….dump";

/// "27 Oct 2026, 03:15 UTC".
fn backup_time(backup: &Backup) -> String {
    format!(
        "{}, {} UTC",
        short_date(backup.created_at),
        backup.created_at.format("%H:%M")
    )
}

/// The schedule typed in the form, if both numbers are in range.
fn parse_schedule(every: &str, keep: &str) -> Option<BackupSchedule> {
    let schedule = BackupSchedule {
        every_days: every.trim().parse().ok()?,
        keep: keep.trim().parse().ok()?,
    };
    schedule.check().ok().map(|()| schedule)
}

#[component]
pub fn BackupsTab() -> Element {
    let mut data = use_signal(|| None::<Backups>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.backups().await {
                Ok(found) => data.set(Some(found)),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });
    let back_up = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.create_backup().await {
                Ok(made) => {
                    if let Some(d) = data.write().as_mut() {
                        d.items.insert(0, made);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let remove = move |name: String| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.delete_backup(&name).await {
                Ok(()) => {
                    if let Some(d) = data.write().as_mut() {
                        d.items.retain(|b| b.name != name);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let client = store::client();
    let link = move |url: &str| client.as_ref().map(|c| c.absolute(url)).unwrap_or_default();

    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("backups-title")} }
            p { class: "muted hint", {t!("backups-lead")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            match data.read().as_ref() {
                None if error.read().is_none() => rsx! { Spinner {} },
                None => rsx! {},
                Some(d) => rsx! {
                    if !d.available {
                        div { class: "banner info", role: "status", {t!("backups-unavailable")} }
                    }
                    if d.items.is_empty() {
                        p { class: "muted", {t!("backups-none")} }
                    } else {
                        ul { class: "sessions",
                            for b in d.items.iter() {
                                li { key: "{b.name}", class: "session",
                                    div { class: "session-main",
                                        strong { {backup_time(b)} }
                                        if b.automatic {
                                            span { class: "pill", {t!("backups-automatic")} }
                                        }
                                        span { class: "muted break",
                                            {format!("{} · {}", human_size(i64::try_from(b.bytes).unwrap_or(i64::MAX)), b.name)}
                                        }
                                    }
                                    div { class: "backup-actions",
                                        a {
                                            class: "icon-btn",
                                            href: link(&b.url),
                                            download: "{b.name}",
                                            title: t!("backups-download"),
                                            aria_label: t!("backups-download"),
                                            Ico { icon: Icon::Download }
                                        }
                                        BackupDelete { name: b.name.clone(), disabled: busy(), ondelete: remove }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "row-end",
                        button {
                            class: "btn",
                            r#type: "button",
                            disabled: busy() || d.running || !d.available,
                            onclick: back_up,
                            Ico { icon: Icon::History }
                            if busy() || d.running { {t!("backups-running")} } else { {t!("backups-now")} }
                        }
                    }
                },
            }
        }
        if let Some(schedule) = data.read().as_ref().map(|d| d.schedule) {
            ScheduleCard {
                schedule,
                onsaved: move |s: BackupSchedule| {
                    let Some(client) = store::client() else {
                        return;
                    };
                    if let Some(d) = data.write().as_mut() {
                        d.schedule = s;
                    }
                    // Saving can remove old automatic backups.
                    spawn(async move {
                        if let Ok(found) = client.backups().await {
                            data.set(Some(found));
                        }
                    });
                },
            }
        }
        section { class: "card",
            h2 { class: "card-title", {t!("backups-restore-title")} }
            p { class: "muted hint", {t!("backups-restore")} }
            pre { class: "command", code { "{RESTORE_COMMAND}" } }
            p { class: "muted hint", {t!("backups-restore-note")} }
        }
    }
}

#[component]
fn ScheduleCard(schedule: BackupSchedule, onsaved: EventHandler<BackupSchedule>) -> Element {
    let mut every = use_signal(|| schedule.every_days.to_string());
    let mut keep = use_signal(|| schedule.keep.to_string());
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut saved = use_signal(|| false);
    let parsed = parse_schedule(&every(), &keep());
    let save = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let Some(schedule) = parse_schedule(&every.peek(), &keep.peek()) else {
            return;
        };
        busy.set(true);
        error.set(None);
        saved.set(false);
        spawn(async move {
            match client.set_backup_schedule(&schedule).await {
                Ok(s) => {
                    every.set(s.every_days.to_string());
                    keep.set(s.keep.to_string());
                    saved.set(true);
                    onsaved.call(s);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("backups-schedule-title")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            form { onsubmit: save, oninput: move |_| saved.set(false),
                div { class: "schedule-fields",
                    label { class: "field",
                        span { {t!("backups-every")} }
                        input {
                            r#type: "number",
                            inputmode: "numeric",
                            min: "0",
                            max: "{MAX_BACKUP_DAYS}",
                            aria_invalid: if parsed.is_some() { "false" } else { "true" },
                            value: "{every}",
                            oninput: move |e| every.set(e.value()),
                        }
                        span { class: "muted field-hint", {t!("backups-every-hint")} }
                    }
                    label { class: "field",
                        span { {t!("backups-keep")} }
                        input {
                            r#type: "number",
                            inputmode: "numeric",
                            min: "1",
                            max: "{MAX_BACKUP_KEEP}",
                            aria_invalid: if parsed.is_some() { "false" } else { "true" },
                            value: "{keep}",
                            oninput: move |e| keep.set(e.value()),
                        }
                        span { class: "muted field-hint", {t!("backups-keep-hint")} }
                    }
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
                        disabled: busy() || parsed.is_none(),
                        {t!("admin-save")}
                    }
                }
            }
        }
    }
}

#[component]
fn BackupDelete(name: String, disabled: bool, ondelete: EventHandler<String>) -> Element {
    let mut confirm = use_signal(|| false);
    let label = t!("backups-delete-named", name = name.clone());
    rsx! {
        button {
            class: if confirm() { "btn danger small" } else { "icon-btn danger" },
            title: t!("backups-delete"),
            aria_label: label,
            disabled,
            onblur: move |_| confirm.set(false),
            onclick: move |_| {
                if confirm() {
                    ondelete.call(name.clone());
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
    use chrono::DateTime;

    use super::*;

    #[test]
    fn schedule_form() {
        assert_eq!(
            parse_schedule(" 1 ", "7"),
            Some(BackupSchedule {
                every_days: 1,
                keep: 7
            })
        );
        assert!(parse_schedule("0", "1").is_some());
        assert_eq!(parse_schedule("", "7"), None);
        assert_eq!(parse_schedule("1", "0"), None);
        assert_eq!(parse_schedule("-1", "7"), None);
        assert_eq!(parse_schedule("1000", "7"), None);
        assert_eq!(parse_schedule("1", "1000"), None);
    }

    #[test]
    fn describes_backups() {
        let backup = Backup {
            name: "starbooru-20270115-080000.dump".into(),
            bytes: 1,
            created_at: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
            automatic: false,
            url: String::new(),
        };
        assert_eq!(backup_time(&backup), "15 Jan 2027, 08:00 UTC");
    }
}
