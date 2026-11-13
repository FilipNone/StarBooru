//! Other boorus: search Danbooru, Gelbooru, Moebooru, e621 or Blombooru
//! sites through the server, pick posts and import them with their tags,
//! rating and source.

use std::collections::{HashMap, HashSet};

use crate::link::Link;
use dioxus::prelude::*;
use starbooru_core::tag::{MAX_TAGS_PER_POST, TagError};
use starbooru_core::{
    BooruApi, ImportJob, ImportRequest, JobState, RemotePost, RemoteSearch, SITES, parse_tag_list,
};

use super::{Choice, ErrorBanner, Select, Spinner};
use crate::Route;
use crate::activity::{self, JOBS};
use crate::icons::{Ico, Icon};
use crate::store::{self, PREFS, SavedBooru};

/// The site picker's value for an address typed by hand.
const CUSTOM: &str = "custom";
/// Sites remembered with their accounts.
const MAX_SAVED: usize = 20;

#[derive(Clone, PartialEq)]
enum Status {
    Busy,
    Done(i64),
    /// Already in the collection (the server's message says which post).
    Known(String),
    Failed(String),
}

/// How a result's import job is going.
fn job_status(job: &ImportJob) -> Status {
    let message = job.error.clone().unwrap_or_default();
    match (job.state, job.post_id) {
        (JobState::Queued | JobState::Running, _) => Status::Busy,
        (JobState::Done, Some(id)) => Status::Done(id),
        // imported, then deleted
        (JobState::Done, None) | (JobState::Duplicate, _) => Status::Known(message),
        (JobState::Failed, _) => Status::Failed(message),
    }
}

pub fn api_of(name: &str) -> Option<BooruApi> {
    BooruApi::ALL.into_iter().find(|api| api.as_str() == name)
}

/// The site to start with: the last one searched, else the first preset.
fn last_site(saved: &[SavedBooru]) -> (BooruApi, String) {
    saved
        .first()
        .and_then(|s| Some((api_of(&s.api)?, s.base.clone())))
        .or_else(|| SITES.first().map(|s| (s.api, s.base.to_owned())))
        .unwrap_or((BooruApi::Danbooru, String::new()))
}

/// The saved account for `base`, as (login, key).
fn account_for(saved: &[SavedBooru], base: &str) -> (String, String) {
    saved
        .iter()
        .find(|s| s.base == base)
        .map(|s| (s.login.clone(), s.api_key.clone()))
        .unwrap_or_default()
}

/// Puts `site` first, replacing the entry for the same address.
fn remember(saved: &mut Vec<SavedBooru>, site: SavedBooru) {
    saved.retain(|s| s.base != site.base);
    saved.insert(0, site);
    saved.truncate(MAX_SAVED);
}

/// A post's own tags with the user's extra ones first, so they survive the
/// per-post limit.
fn import_tags(post: &[String], extra: &str) -> Result<Vec<String>, TagError> {
    let mut tags = parse_tag_list(extra)?;
    for tag in post {
        if tags.len() >= MAX_TAGS_PER_POST {
            break;
        }
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    Ok(tags)
}

/// The picker's value for a site: its preset's address, or `CUSTOM`.
fn picker_value(api: BooruApi, base: &str) -> String {
    SITES
        .iter()
        .find(|s| s.api == api && s.base == base)
        .map_or_else(|| CUSTOM.to_owned(), |s| s.base.to_owned())
}

#[component]
pub fn Boorus() -> Element {
    let start = last_site(&PREFS.peek().boorus);
    let start_account = account_for(&PREFS.peek().boorus, &start.1);
    let mut api = use_signal(|| start.0);
    let mut base = use_signal(|| start.1.clone());
    let mut custom = use_signal(|| picker_value(start.0, &start.1) == CUSTOM);
    let mut login = use_signal(|| start_account.0.clone());
    let mut api_key = use_signal(|| start_account.1.clone());
    let mut query = use_signal(String::new);
    let mut posts = use_signal(Vec::<RemotePost>::new);
    let mut page = use_signal(|| 1u32);
    let mut more = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut searched = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut selected = use_signal(HashSet::<u64>::new);
    let mut status = use_signal(HashMap::<u64, Status>::new);
    let mut extra = use_signal(String::new);
    let mut importing = use_signal(|| false);

    let mut reset = move || {
        posts.write().clear();
        selected.write().clear();
        status.write().clear();
        more.set(false);
        searched.set(false);
        error.set(None);
    };
    let mut choose = move |value: String| {
        reset();
        if value == CUSTOM {
            custom.set(true);
            return;
        }
        custom.set(false);
        if let Some(site) = SITES.iter().find(|s| s.base == value) {
            api.set(site.api);
            base.set(site.base.to_owned());
            let (l, k) = account_for(&PREFS.peek().boorus, site.base);
            login.set(l);
            api_key.set(k);
        }
    };

    let mut run = move |next: u32| {
        if loading() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let search = RemoteSearch {
            api: api(),
            base: base(),
            tags: query(),
            page: next,
            login: Some(login()),
            api_key: Some(api_key()),
        };
        loading.set(true);
        error.set(None);
        spawn(async move {
            match client.remote_search(&search).await {
                Ok(found) => {
                    remember(
                        &mut PREFS.write().boorus,
                        SavedBooru {
                            api: search.api.as_str().to_owned(),
                            base: search.base.trim().trim_end_matches('/').to_owned(),
                            login: search.login.unwrap_or_default().trim().to_owned(),
                            api_key: search.api_key.unwrap_or_default().trim().to_owned(),
                        },
                    );
                    let mut list = posts.write();
                    if next == 1 {
                        list.clear();
                        selected.write().clear();
                    }
                    for post in found.posts {
                        if !list.iter().any(|p| p.id == post.id) {
                            list.push(post);
                        }
                    }
                    more.set(found.more);
                    page.set(next);
                    searched.set(true);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };

    let import = move |e: FormEvent| {
        e.prevent_default();
        if importing() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let extra = extra();
        if let Err(e) = parse_tag_list(&extra) {
            error.set(Some(e.to_string()));
            return;
        }
        let picked: Vec<RemotePost> = posts
            .read()
            .iter()
            .filter(|p| selected.read().contains(&p.id))
            .cloned()
            .collect();
        let mut requests = Vec::new();
        let mut sent = Vec::new();
        for post in picked {
            match import_tags(&post.tags, &extra) {
                Err(e) => {
                    status
                        .write()
                        .insert(post.id, Status::Failed(e.to_string()));
                }
                Ok(tags) => {
                    sent.push(post.id);
                    requests.push(ImportRequest {
                        url: post.file_url,
                        tags,
                        rating: post.rating,
                        source: Some(post.page_url),
                    });
                }
            }
        }
        if requests.is_empty() {
            return;
        }
        importing.set(true);
        error.set(None);
        // The server runs the imports; their progress comes from `JOBS`.
        spawn(async move {
            match activity::queue_imports(&client, requests).await {
                Ok(()) => {
                    for id in sent {
                        status.write().remove(&id);
                        selected.write().remove(&id);
                    }
                }
                Err(message) => error.set(Some(message)),
            }
            importing.set(false);
        });
    };

    let picker = if custom() {
        CUSTOM.to_owned()
    } else {
        picker_value(api(), &base())
    };
    let count = selected.read().len();
    let selecting = count > 0;
    let jobs = activity::jobs_by_source(&JOBS.read());
    let shown: HashMap<u64, Status> = posts
        .read()
        .iter()
        .filter_map(|p| {
            let local = status.read().get(&p.id).cloned();
            Some((
                p.id,
                local.or_else(|| jobs.get(&p.page_url).map(job_status))?,
            ))
        })
        .collect();
    let failures: Vec<String> = shown
        .values()
        .filter_map(|s| match s {
            Status::Failed(m) => Some(m.clone()),
            _ => None,
        })
        .collect();
    let busy = shown.values().any(|s| *s == Status::Busy);
    rsx! {
        div { class: "page boorus",
            h1 { class: "page-title", {t!("boorus-title")} }
            p { class: "muted boorus-hint", {t!("boorus-hint")} }
            div { class: "card boorus-site",
                div { class: "boorus-fields",
                    div { class: "field",
                        span { aria_hidden: "true", {t!("boorus-site")} }
                        Select {
                            class: "boxed",
                            aria_label: t!("boorus-site"),
                            value: picker.to_string(),
                            options: SITES
                                .iter()
                                .map(|s| Choice::new(s.base, s.name))
                                .chain(std::iter::once(Choice::new(CUSTOM, t!("boorus-custom"))))
                                .collect::<Vec<_>>(),
                            onchange: move |v: String| choose(v),
                        }
                    }
                    if custom() {
                        label { class: "field",
                            span { {t!("boorus-address")} }
                            input {
                                r#type: "url",
                                placeholder: "https://booru.example",
                                spellcheck: "false",
                                autocapitalize: "none",
                                value: "{base}",
                                oninput: move |e| {
                                    base.set(e.value());
                                    let (l, k) = account_for(&PREFS.peek().boorus, e.value().trim().trim_end_matches('/'));
                                    login.set(l);
                                    api_key.set(k);
                                },
                            }
                        }
                        div { class: "field",
                            span { aria_hidden: "true", {t!("boorus-api")} }
                            Select {
                                class: "boxed",
                                aria_label: t!("boorus-api"),
                                value: api().as_str().to_string(),
                                options: BooruApi::ALL.into_iter().map(|a| Choice::new(a.as_str(), a.name())).collect::<Vec<_>>(),
                                onchange: move |v: String| {
                                    if let Some(a) = api_of(&v) {
                                        api.set(a);
                                    }
                                },
                            }
                        }
                    }
                }
                details { class: "boorus-account", open: !login().is_empty() || !api_key().is_empty(),
                    summary { {t!("boorus-account")} }
                    p { class: "muted hint", {t!("boorus-account-hint")} }
                    div { class: "boorus-fields",
                        label { class: "field",
                            span {
                                if api() == BooruApi::Gelbooru { {t!("boorus-user-id")} } else { {t!("boorus-login")} }
                            }
                            input {
                                autocomplete: "off",
                                spellcheck: "false",
                                autocapitalize: "none",
                                value: "{login}",
                                oninput: move |e| login.set(e.value()),
                            }
                        }
                        label { class: "field",
                            span {
                                if api() == BooruApi::Moebooru { {t!("boorus-password-hash")} } else { {t!("boorus-key")} }
                            }
                            input {
                                r#type: "password",
                                autocomplete: "off",
                                spellcheck: "false",
                                value: "{api_key}",
                                oninput: move |e| api_key.set(e.value()),
                            }
                        }
                    }
                }
            }
            form {
                class: "inline-add boorus-search",
                role: "search",
                onsubmit: move |e| {
                    e.prevent_default();
                    run(1);
                },
                input {
                    r#type: "search",
                    placeholder: t!("boorus-search-placeholder"),
                    aria_label: t!("boorus-search-label"),
                    spellcheck: "false",
                    autocapitalize: "none",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
                button { class: "btn", r#type: "submit", disabled: loading() || base().trim().is_empty(),
                    Ico { icon: Icon::Search }
                    {t!("boorus-search")}
                }
            }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if busy {
                div { class: "banner info", role: "status", {t!("boorus-queued")} }
            }
            for message in failures.iter().take(1) {
                div { class: "banner warn", role: "status", {t!("boorus-some-failed", error = message.clone())} }
            }
            if searched() && posts.read().is_empty() && !loading() {
                div { class: "card empty",
                    Ico { icon: Icon::Search, size: 40 }
                    p { {t!("boorus-nothing")} }
                }
            }
            div { class: if selecting { "grid selecting" } else { "grid" },
                for (index, post) in posts.read().iter().enumerate() {
                    RemoteThumb {
                        key: "{post.id}",
                        post: post.clone(),
                        delay: (index % 40) * 15,
                        on: selected.read().contains(&post.id),
                        status: shown.get(&post.id).cloned(),
                        ontoggle: move |id| {
                            let mut set = selected.write();
                            if !set.remove(&id) {
                                set.insert(id);
                            }
                        },
                    }
                }
            }
            if loading() {
                div { class: "sentinel", Spinner {} }
            } else if more() {
                div { class: "sentinel",
                    button { class: "btn tonal", onclick: move |_| run(page().saturating_add(1)), {t!("boorus-more")} }
                }
            }
            if selecting {
                form { class: "bulk-bar glass", aria_label: t!("boorus-import-label"), onsubmit: import,
                    div { class: "bulk-count",
                        strong { role: "status", {t!("bulk-selected", count = count)} }
                        button {
                            r#type: "button",
                            class: "btn text small",
                            onclick: move |_| selected.write().clear(),
                            {t!("bulk-select-none")}
                        }
                    }
                    input {
                        class: "bulk-tags",
                        r#type: "text",
                        placeholder: t!("boorus-extra-tags"),
                        aria_label: t!("boorus-extra-tags"),
                        autocapitalize: "none",
                        spellcheck: false,
                        value: "{extra}",
                        oninput: move |e| extra.set(e.value()),
                    }
                    button { class: "btn", r#type: "submit", disabled: importing(),
                        Ico { icon: Icon::Download }
                        {t!("boorus-import", count = count)}
                    }
                }
            }
        }
    }
}

/// One result: a toggle while it can be picked, a link once imported.
#[component]
fn RemoteThumb(
    post: RemotePost,
    delay: usize,
    on: bool,
    status: Option<Status>,
    ontoggle: EventHandler<u64>,
) -> Element {
    let alt = post
        .tags
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let media = rsx! {
        div { class: "thumb-media",
            img {
                src: "{post.preview_url}",
                alt: "{alt}",
                loading: "lazy",
                decoding: "async",
                "referrerpolicy": "no-referrer",
            }
        }
    };
    let pill = match &status {
        None => rsx! {},
        Some(Status::Busy) => {
            rsx! { span { class: "pill busy remote-status", {t!("upload-uploading")} } }
        }
        Some(Status::Done(id)) => {
            rsx! { span { class: "pill ok remote-status", {t!("post-number", id = id.to_string())} } }
        }
        Some(Status::Known(message)) => {
            rsx! { span { class: "pill remote-status", title: "{message}", {t!("boorus-known")} } }
        }
        Some(Status::Failed(message)) => {
            rsx! { span { class: "pill bad remote-status", title: "{message}", {t!("upload-failed")} } }
        }
    };
    if let Some(Status::Done(id)) = status {
        return rsx! {
            Link {
                class: "thumb",
                style: "--delay: {delay}ms",
                to: Route::PostView { id, q: String::new() },
                {media}
                {pill}
            }
        };
    }
    let id = post.id;
    rsx! {
        button {
            class: if on { "thumb selectable on" } else { "thumb selectable" },
            style: "--delay: {delay}ms",
            r#type: "button",
            aria_pressed: on,
            aria_label: if alt.is_empty() { t!("boorus-post", id = id.to_string()) } else { alt.clone() },
            disabled: status == Some(Status::Busy),
            onclick: move |_| ontoggle.call(id),
            {media}
            span { class: "check-mark", aria_hidden: "true", Ico { icon: Icon::Check, size: 20 } }
            {pill}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(api: &str, base: &str, login: &str) -> SavedBooru {
        SavedBooru {
            api: api.into(),
            base: base.into(),
            login: login.into(),
            api_key: format!("{login}-key"),
        }
    }

    #[test]
    fn starts_with_the_last_site() {
        let first = SITES.first().unwrap();
        assert_eq!(last_site(&[]), (first.api, first.base.to_owned()));
        let list = [saved("e621", "https://e926.net", "me")];
        assert_eq!(
            last_site(&list),
            (BooruApi::E621, "https://e926.net".to_owned())
        );
        // An API this version doesn't know falls back to the presets.
        let list = [saved("szurubooru", "https://s.org", "me")];
        assert_eq!(last_site(&list).0, first.api);
    }

    #[test]
    fn remembers_accounts_by_address() {
        let mut list = vec![saved("danbooru", "https://a.org", "a")];
        remember(&mut list, saved("gelbooru", "https://b.org", "b"));
        remember(&mut list, saved("danbooru", "https://a.org", "a2"));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].login, "a2");
        assert_eq!(
            account_for(&list, "https://b.org"),
            ("b".into(), "b-key".into())
        );
        assert_eq!(account_for(&list, "https://c.org"), Default::default());
        for i in 0..MAX_SAVED + 5 {
            remember(
                &mut list,
                saved("danbooru", &format!("https://{i}.org"), ""),
            );
        }
        assert_eq!(list.len(), MAX_SAVED);
    }

    #[test]
    fn extra_tags_come_first_and_fit() {
        let post = vec!["sky".to_owned(), "sea".to_owned()];
        assert_eq!(
            import_tags(&post, "From_Danbooru sky").unwrap(),
            ["from_danbooru", "sky", "sea"]
        );
        let full: Vec<String> = (0..MAX_TAGS_PER_POST).map(|i| format!("t{i}")).collect();
        let tags = import_tags(&full, "mine").unwrap();
        assert_eq!(tags.len(), MAX_TAGS_PER_POST);
        assert_eq!(tags[0], "mine");
        assert!(import_tags(&post, "~bad").is_err());
    }

    #[test]
    fn presets_show_in_the_picker() {
        for site in SITES {
            assert_eq!(picker_value(site.api, site.base), site.base);
        }
        assert_eq!(
            picker_value(BooruApi::Danbooru, "https://safebooru.org"),
            CUSTOM
        );
        assert_eq!(picker_value(BooruApi::Blombooru, "http://nas:8000"), CUSTOM);
    }
}
