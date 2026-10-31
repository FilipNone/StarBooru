//! Settings › Import from another booru: moves a whole library over. The server
//! does the work, so it goes on when this app is closed; this card starts,
//! follows and repairs those imports.

use dioxus::prelude::*;
use starbooru_core::{
    LibraryCheck, LibraryError, LibraryFailure, LibraryImport, LibraryPhase, LibraryRetry,
    LibrarySite, LibrarySource, LibraryState, MAX_ACTIVE_LIBRARY_IMPORTS, Rating,
    clean_credentials,
};

use super::{Choice, ErrorBanner, Select, Spinner};
use crate::activity::wait_ms;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{rating_label, time_ago};

/// How often a running import's numbers are read again, in milliseconds.
const POLL_MS: u32 = 3000;
/// When nothing is running, one read in this many, for imports started elsewhere.
const IDLE_ROUNDS: u32 = 10;

/// Where this system keeps what the form asks for, and what doesn't come across.
fn source_hint(source: LibrarySource) -> String {
    match source {
        LibrarySource::Szurubooru => t!("library-hint-szurubooru"),
        LibrarySource::Shimmie2 => t!("library-hint-shimmie2"),
        LibrarySource::Danbooru => t!("library-hint-danbooru"),
        LibrarySource::Stash => t!("library-hint-stash"),
        LibrarySource::Blombooru => t!("library-hint-blombooru"),
        LibrarySource::Monbooru => t!("library-hint-monbooru"),
        LibrarySource::Lanbooru => t!("library-hint-lanbooru"),
    }
}

/// An empty field means "not given".
fn given(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn form_error(error: &LibraryError) -> String {
    match error {
        LibraryError::Base(_) => t!("library-bad-address"),
        LibraryError::MissingLogin(_) => t!("library-need-login"),
        LibraryError::MissingSecret(_) => t!("library-need-key"),
    }
}

fn state_label(import: &LibraryImport) -> String {
    match import.state {
        LibraryState::Queued => t!("library-state-queued"),
        LibraryState::Running => match import.phase {
            LibraryPhase::Rules => t!("library-state-rules"),
            LibraryPhase::Posts => t!("library-state-posts"),
            LibraryPhase::Groups => t!("library-state-groups"),
        },
        LibraryState::Done => t!("library-state-done"),
        LibraryState::Stopped => t!("library-state-stopped"),
        LibraryState::Failed => t!("library-state-failed"),
    }
}

/// "12 of 300 · 2 already here · 1 failed · 40 aliases and implications".
fn counts(import: &LibraryImport) -> String {
    let mut parts = vec![match import.total {
        Some(total) => t!("library-count-of", done = import.handled(), total = total),
        None => t!("library-count", done = import.handled()),
    }];
    if import.merged > 0 {
        parts.push(t!("library-count-merged", count = import.merged));
    }
    if import.failed > 0 {
        parts.push(t!("library-count-failed", count = import.failed));
    }
    if import.rules > 0 {
        parts.push(t!("library-count-rules", count = import.rules));
    }
    parts.join(" · ")
}

#[component]
pub fn LibraryImportCard() -> Element {
    let mut list = use_signal(|| None::<Vec<LibraryImport>>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut checking = use_signal(|| false);
    let mut source = use_signal(|| LibrarySource::Szurubooru);
    let mut base = use_signal(String::new);
    let mut login = use_signal(String::new);
    let mut secret = use_signal(String::new);
    let mut unrated = use_signal(|| Rating::Questionable);
    let mut checked = use_signal(|| None::<LibraryCheck>);

    // Goes on as long as the card is on screen, reading often only while
    // something is moving.
    use_future(move || async move {
        loop {
            let Some(client) = store::client() else {
                return;
            };
            match client.library_imports().await {
                Ok(found) => list.set(Some(found)),
                Err(e) if e.is_unauthorized() => {
                    store::forget_session();
                    return;
                }
                // The next read tries again.
                Err(_) => {}
            }
            for _ in 0..IDLE_ROUNDS {
                wait_ms(POLL_MS).await;
                let moving = list.peek().iter().flatten().any(|i| i.state.is_active());
                if moving {
                    break;
                }
            }
        }
    });

    let chosen = source();
    let form = move || -> Result<LibrarySite, String> {
        let site = LibrarySite {
            source: source(),
            base: base(),
            login: given(&login()).filter(|_| source().takes_login()),
            secret: given(&secret()).filter(|_| source().takes_secret()),
            unrated: unrated(),
        };
        site.clean().map_err(|e| form_error(&e))
    };
    let check = move |_| {
        let site = match form() {
            Ok(site) => site,
            Err(e) => return error.set(Some(e)),
        };
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        checking.set(true);
        error.set(None);
        checked.set(None);
        spawn(async move {
            match client.check_library(&site).await {
                Ok(found) => checked.set(Some(found)),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
            checking.set(false);
        });
    };
    let start = move |e: FormEvent| {
        e.prevent_default();
        let site = match form() {
            Ok(site) => site,
            Err(e) => return error.set(Some(e)),
        };
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.start_library_import(&site).await {
                Ok(import) => {
                    list.write().get_or_insert_default().insert(0, import);
                    base.set(String::new());
                    login.set(String::new());
                    secret.set(String::new());
                    checked.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let sources: Vec<Choice> = LibrarySource::ALL
        .into_iter()
        .map(|s| Choice::new(s.as_str(), s.name()))
        .collect();
    let ratings: Vec<Choice> = Rating::ALL
        .into_iter()
        .map(|r| Choice::new(r.as_str(), rating_label(r)))
        .collect();
    let active = list
        .read()
        .iter()
        .flatten()
        .filter(|i| i.state.is_active())
        .count();
    let full = i64::try_from(active).unwrap_or(i64::MAX) >= MAX_ACTIVE_LIBRARY_IMPORTS;
    let ready = !busy() && !base().trim().is_empty() && !full;

    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("library-title")} }
            p { class: "muted hint", {t!("library-lead")} }
            p { class: "muted hint", {t!("library-left-out")} }

            form { class: "key-form library-form", onsubmit: start,
                div { class: "field",
                    span { {t!("library-source")} }
                    Select {
                        value: chosen.as_str().to_owned(),
                        options: sources,
                        aria_label: t!("library-source"),
                        onchange: move |value: String| {
                            if let Some(picked) = LibrarySource::parse(&value) {
                                source.set(picked);
                                checked.set(None);
                                error.set(None);
                            }
                        },
                    }
                }
                p { class: "muted hint", {source_hint(chosen)} }
                label { class: "field",
                    span { {t!("boorus-address")} }
                    input {
                        r#type: "url",
                        inputmode: "url",
                        autocapitalize: "none",
                        spellcheck: false,
                        placeholder: "http://192.168.1.4:8080",
                        value: "{base}",
                        oninput: move |e| {
                            base.set(e.value());
                            checked.set(None);
                        },
                    }
                }
                if chosen.takes_login() {
                    label { class: "field",
                        span { {t!("boorus-login")} }
                        input {
                            autocomplete: "off",
                            autocapitalize: "none",
                            spellcheck: false,
                            value: "{login}",
                            oninput: move |e| login.set(e.value()),
                        }
                    }
                }
                if chosen.takes_secret() {
                    label { class: "field",
                        span { {t!("boorus-key")} }
                        input {
                            r#type: "password",
                            autocomplete: "off",
                            value: "{secret}",
                            oninput: move |e| secret.set(e.value()),
                        }
                    }
                }
                div { class: "field",
                    span { {t!("library-unrated")} }
                    Select {
                        value: unrated().as_str().to_owned(),
                        options: ratings,
                        aria_label: t!("library-unrated"),
                        onchange: move |value: String| {
                            if let Some(picked) = Rating::parse(&value) {
                                unrated.set(picked);
                            }
                        },
                    }
                }
                if let Some(message) = error() {
                    ErrorBanner { message }
                }
                if let Some(found) = checked() {
                    div { class: "banner info", role: "status",
                        match found.total {
                            Some(total) => t!("library-found", count = total),
                            None => t!("library-found-some"),
                        }
                        if !found.rules {
                            " "
                            {t!("library-no-rules")}
                        }
                    }
                }
                if full {
                    p { class: "muted hint", {t!("library-full")} }
                }
                div { class: "row-end",
                    button {
                        class: "btn tonal",
                        r#type: "button",
                        disabled: !ready,
                        onclick: check,
                        if checking() {
                            {t!("library-checking")}
                        } else {
                            {t!("library-check")}
                        }
                    }
                    button { class: "btn", r#type: "submit", disabled: !ready,
                        Ico { icon: Icon::Download }
                        {t!("library-start")}
                    }
                }
            }

            match list.read().as_ref() {
                None => rsx! { Spinner {} },
                Some(imports) if imports.is_empty() => rsx! {},
                Some(imports) => rsx! {
                    h3 { class: "library-heading", {t!("library-list")} }
                    ul { class: "sessions library-list",
                        for import in imports.iter().cloned() {
                            ImportRow { key: "{import.id}", import, list }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ImportRow(import: LibraryImport, list: Signal<Option<Vec<LibraryImport>>>) -> Element {
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut asking = use_signal(|| false);
    let mut login = use_signal(String::new);
    let mut secret = use_signal(String::new);
    let mut failures = use_signal(|| None::<Vec<LibraryFailure>>);

    let id = import.id;
    let source = import.source;
    let mut replace = move |updated: LibraryImport| {
        if let Some(found) = list
            .write()
            .iter_mut()
            .flatten()
            .find(|i| i.id == updated.id)
        {
            *found = updated;
        }
    };
    let stop = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match client.stop_library_import(id).await {
                Ok(updated) => replace(updated),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let mut retry = move || {
        let Some(client) = store::client() else {
            return;
        };
        let account = LibraryRetry {
            login: given(&login()).filter(|_| source.takes_login()),
            secret: given(&secret()).filter(|_| source.takes_secret()),
        };
        if let Err(e) = clean_credentials(source, account.login.clone(), account.secret.clone()) {
            return error.set(Some(form_error(&e)));
        }
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.retry_library_import(id, &account).await {
                Ok(updated) => {
                    replace(updated);
                    asking.set(false);
                    login.set(String::new());
                    secret.set(String::new());
                    failures.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let remove = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match client.delete_library_import(id).await {
                Ok(()) => {
                    if let Some(all) = list.write().as_mut() {
                        all.retain(|i| i.id != id);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let show_failures = move |_| {
        if failures.peek().is_some() {
            return failures.set(None);
        }
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.library_failures(id).await {
                Ok(found) => failures.set(Some(found)),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let active = import.state.is_active();
    let now = chrono::Utc::now();
    let when = time_ago(import.finished_at.unwrap_or(import.created_at), now);
    let takes_account = source.takes_login() || source.takes_secret();

    rsx! {
        li { class: "library-import",
            div { class: "library-import-head",
                strong { {source.name()} }
                span { class: "muted break", "{import.base}" }
            }
            div { class: "library-import-state",
                span {
                    class: match import.state {
                        LibraryState::Failed => "pill bad",
                        LibraryState::Done => "pill ok",
                        _ => "pill",
                    },
                    {state_label(&import)}
                }
                if !active {
                    span { class: "muted", "{when}" }
                }
            }
            if active {
                match import.fraction() {
                    Some(f) => rsx! {
                        span {
                            class: "meter library-meter",
                            role: "meter",
                            aria_valuemin: "0",
                            aria_valuemax: "100",
                            aria_valuenow: "{f * 100.0:.0}",
                            aria_label: state_label(&import),
                            span { style: "width: {f * 100.0:.1}%" }
                        }
                    },
                    None => rsx! {
                        span {
                            class: "meter library-meter unknown",
                            role: "progressbar",
                            aria_label: state_label(&import),
                            span {}
                        }
                    },
                }
            }
            span { class: "muted", {counts(&import)} }
            if let Some(reason) = import.error.as_ref() {
                p { class: "library-reason", "{reason}" }
            }
            if let Some(note) = import.note.as_ref() {
                p { class: "muted hint", "{note}" }
            }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if asking() {
                form {
                    class: "library-account",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        retry();
                    },
                    p { class: "muted hint", {t!("library-account-again")} }
                    if source.takes_login() {
                        label { class: "field",
                            span { {t!("boorus-login")} }
                            input {
                                    autocomplete: "off",
                                autocapitalize: "none",
                                value: "{login}",
                                oninput: move |e| login.set(e.value()),
                            }
                        }
                    }
                    if source.takes_secret() {
                        label { class: "field",
                            span { {t!("boorus-key")} }
                            input {
                                    r#type: "password",
                                autocomplete: "off",
                                value: "{secret}",
                                oninput: move |e| secret.set(e.value()),
                            }
                        }
                    }
                    div { class: "row-end",
                        button {
                            class: "btn tonal small",
                            r#type: "button",
                            onclick: move |_| asking.set(false),
                            {t!("cancel")}
                        }
                        button { class: "btn small", r#type: "submit", disabled: busy(),
                            {t!("library-retry")}
                        }
                    }
                }
            }
            div { class: "row-end library-actions",
                if import.failed > 0 {
                    button {
                        class: "btn tonal small",
                        r#type: "button",
                        aria_expanded: failures.read().is_some(),
                        onclick: show_failures,
                        {t!("library-show-failed")}
                    }
                }
                if active {
                    button {
                        class: "btn tonal small",
                        r#type: "button",
                        disabled: busy(),
                        onclick: stop,
                        {t!("library-stop")}
                    }
                } else {
                    button {
                        class: "btn tonal small",
                        r#type: "button",
                        disabled: busy(),
                        onclick: remove,
                        {t!("library-remove")}
                    }
                    if !asking() {
                        button {
                            class: "btn tonal small",
                            r#type: "button",
                            disabled: busy(),
                            onclick: move |_| {
                                if takes_account {
                                    asking.set(true);
                                } else {
                                    retry();
                                }
                            },
                            {t!("library-retry")}
                        }
                    }
                }
            }
            if let Some(found) = failures.read().as_ref() {
                ul { class: "library-failures",
                    for failure in found {
                        li { key: "{failure.remote_id}",
                            code { "{failure.remote_id}" }
                            " "
                            span { class: "muted", "{failure.error}" }
                        }
                    }
                }
            }
        }
    }
}
