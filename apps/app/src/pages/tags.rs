//! The tag list and the tag page: category, aliases and implications.

use dioxus::prelude::*;
use starbooru_client::ClientError;
use starbooru_core::{Role, Tag, TagCategory, TagDetail, TagOrder, TagUpdate};

use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store::{self, SESSION};

const PAGE_SIZE: u32 = 60;

const CATEGORIES: [TagCategory; 5] = [
    TagCategory::General,
    TagCategory::Artist,
    TagCategory::Character,
    TagCategory::Copyright,
    TagCategory::Meta,
];

fn category_label(category: TagCategory) -> String {
    match category {
        TagCategory::General => t!("category-general"),
        TagCategory::Artist => t!("category-artist"),
        TagCategory::Character => t!("category-character"),
        TagCategory::Copyright => t!("category-copyright"),
        TagCategory::Meta => t!("category-meta"),
    }
}

fn is_admin() -> bool {
    SESSION
        .read()
        .as_ref()
        .is_some_and(|s| s.user.role == Role::Admin)
}

fn report(error: &mut Signal<Option<String>>, e: ClientError) {
    if e.is_unauthorized() {
        store::forget_session();
    } else {
        error.set(Some(e.to_string()));
    }
}

#[component]
pub fn Tags() -> Element {
    let mut tags = use_signal(Vec::<Tag>::new);
    let mut filter = use_signal(String::new);
    let mut order = use_signal(TagOrder::default);
    let mut next_page = use_signal(|| 1u32);
    let mut has_more = use_signal(|| true);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // Bumped whenever the filter or order changes, so stale pages are dropped.
    let mut generation = use_signal(|| 0u64);

    let mut load_more = move || {
        if *loading.peek() || !*has_more.peek() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let generation_at_start = *generation.peek();
        let page = *next_page.peek();
        let prefix = filter.peek().trim().to_owned();
        let order_now = *order.peek();
        loading.set(true);
        spawn(async move {
            let result = client.tag_list(&prefix, order_now, page, PAGE_SIZE).await;
            if *generation.peek() != generation_at_start {
                return;
            }
            loading.set(false);
            match result {
                Ok(found) => {
                    has_more.set(found.len() == PAGE_SIZE as usize);
                    tags.write().extend(found);
                    next_page.set(page.saturating_add(1));
                }
                Err(e) => {
                    has_more.set(false);
                    report(&mut error, e);
                }
            }
        });
    };
    use_hook(load_more);

    let mut restart = move || {
        generation += 1;
        tags.write().clear();
        next_page.set(1);
        has_more.set(true);
        loading.set(false);
        error.set(None);
        load_more();
    };

    let is_empty = tags.read().is_empty() && !loading() && !has_more() && error().is_none();

    rsx! {
        div { class: "page",
            div { class: "page-head",
                h1 { class: "page-title", {t!("nav-tags")} }
                div { class: "segmented", role: "radiogroup", aria_label: t!("tags-sort"),
                    for (value, label) in [(TagOrder::Count, t!("tags-most-used")), (TagOrder::Name, t!("tags-a-z"))] {
                        button {
                            key: "{label}",
                            role: "radio",
                            aria_checked: if order() == value { "true" } else { "false" },
                            class: if order() == value { "seg on" } else { "seg" },
                            onclick: move |_| {
                                if order() != value {
                                    order.set(value);
                                    restart();
                                }
                            },
                            "{label}"
                        }
                    }
                }
            }
            label { class: "field tag-filter",
                span { class: "visually-hidden", {t!("tags-filter-label")} }
                input {
                    r#type: "search",
                    placeholder: t!("tags-filter-placeholder"),
                    autocomplete: "off",
                    spellcheck: "false",
                    value: "{filter}",
                    oninput: move |e| {
                        filter.set(e.value());
                        restart();
                    },
                }
            }
            if let Some(message) = error() {
                ErrorBanner { message, onretry: move |_| restart() }
            }
            if is_empty {
                div { class: "card empty",
                    Ico { icon: Icon::Label, size: 48 }
                    h2 {
                        if filter().trim().is_empty() { {t!("tags-none-yet")} } else { {t!("tags-none-match")} }
                    }
                    p { {t!("tags-appear")} }
                }
            }
            if !tags.read().is_empty() {
                ul { class: "tag-list card",
                    for tag in tags.read().iter() {
                        li { key: "{tag.id}",
                            Link {
                                class: "tag-row",
                                to: Route::TagPage { name: tag.name.clone() },
                                span { class: "tag-name", "{tag.name}" }
                                span { class: "pill", "{category_label(tag.category)}" }
                                span { class: "count", "{tag.post_count}" }
                            }
                        }
                    }
                }
            }
            if has_more() && error().is_none() {
                div {
                    class: "sentinel",
                    onvisible: move |e| {
                        if e.is_intersecting().unwrap_or(false) {
                            load_more();
                        }
                    },
                    if loading() {
                        Spinner {}
                    } else {
                        button { class: "btn tonal", onclick: move |_| load_more(), {t!("load-more")} }
                    }
                }
            }
        }
    }
}

/// Which relation an edit on the tag page touches.
#[derive(Clone, Copy, PartialEq)]
enum Relation {
    Alias,
    Implies,
}

#[component]
pub fn TagPage(name: String) -> Element {
    let mut detail = use_signal(|| None::<TagDetail>);
    let mut error = use_signal(|| None::<String>);
    let mut missing = use_signal(|| false);
    let mut busy = use_signal(|| false);

    let load = move |name: String| {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.tag(&name).await {
                Ok(d) => {
                    missing.set(false);
                    detail.set(Some(d));
                }
                Err(ClientError::Api { status: 404, .. }) => missing.set(true),
                Err(e) => report(&mut error, e),
            }
        });
    };
    use_effect(use_reactive!(|name| {
        detail.set(None);
        error.set(None);
        load(name);
    }));

    let mut apply = move |result: Result<TagDetail, ClientError>| match result {
        Ok(d) => {
            error.set(None);
            detail.set(Some(d));
        }
        Err(e) => report(&mut error, e),
    };

    let mut set_category = move |category: TagCategory| {
        let (Some(client), Some(tag)) = (store::client(), detail.peek().clone()) else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match client
                .update_tag(&tag.tag.name, &TagUpdate { category })
                .await
            {
                Ok(updated) => {
                    if let Some(d) = detail.write().as_mut() {
                        d.tag = updated;
                    }
                }
                Err(e) => report(&mut error, e),
            }
            busy.set(false);
        });
    };

    let add = move |(relation, value): (Relation, String)| {
        let (Some(client), Some(tag)) = (store::client(), detail.peek().clone()) else {
            return;
        };
        busy.set(true);
        spawn(async move {
            let name = &tag.tag.name;
            apply(match relation {
                Relation::Alias => client.add_alias(name, &value).await,
                Relation::Implies => client.add_implication(name, &value).await,
            });
            busy.set(false);
        });
    };

    let remove = move |(relation, value): (Relation, String)| {
        let (Some(client), Some(tag)) = (store::client(), detail.peek().clone()) else {
            return;
        };
        busy.set(true);
        spawn(async move {
            let name = &tag.tag.name;
            apply(match relation {
                Relation::Alias => client.remove_alias(name, &value).await,
                Relation::Implies => client.remove_implication(name, &value).await,
            });
            busy.set(false);
        });
    };

    let admin = is_admin();
    let Some(d) = detail() else {
        return rsx! {
            div { class: "page",
                if let Some(message) = error() {
                    ErrorBanner { message, onretry: move |_| load(name.clone()) }
                } else if missing() {
                    div { class: "card empty",
                        Ico { icon: Icon::Label, size: 48 }
                        h2 { {t!("tags-missing", name = name.clone())} }
                        Link { class: "btn tonal", to: Route::Tags {}, {t!("tags-all")} }
                    }
                } else {
                    div { class: "sentinel", Spinner {} }
                }
            }
        };
    };
    let tag = d.tag.clone();
    let posts_label = t!("tags-browse", count = tag.post_count);

    rsx! {
        div { class: "page tag-page",
            div { class: "page-head",
                h1 { class: "page-title break", "{tag.name}" }
                Link { class: "btn tonal", to: Route::Gallery { q: tag.name.clone() },
                    Ico { icon: Icon::Gallery }
                    "{posts_label}"
                }
            }
            if tag.name != name.trim().to_lowercase() {
                div { class: "banner info", role: "status",
                    {t!("tags-is-alias", name = name.clone())}
                }
            }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            div { class: "tag-cards",
                section { class: "card",
                    h2 { class: "card-title", {t!("tags-category")} }
                    div { class: "chips", role: "radiogroup", aria_label: t!("tags-category"),
                        for (category, label) in CATEGORIES.map(|c| (c, category_label(c))) {
                            button {
                                key: "{label}",
                                role: "radio",
                                disabled: busy(),
                                aria_checked: if tag.category == category { "true" } else { "false" },
                                class: if tag.category == category { "chip on" } else { "chip" },
                                onclick: move |_| set_category(category),
                                "{label}"
                            }
                        }
                    }
                }
                RelationCard {
                    title: t!("tags-aliases"),
                    hint: t!("tags-aliases-hint"),
                    empty: t!("tags-aliases-none"),
                    names: d.aliases.clone(),
                    linked: false,
                    relation: Relation::Alias,
                    admin,
                    busy: busy(),
                    onadd: add,
                    onremove: remove,
                }
                RelationCard {
                    title: t!("tags-implies"),
                    hint: t!("tags-implies-hint"),
                    empty: t!("tags-implies-none"),
                    names: d.implies.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
                    linked: true,
                    relation: Relation::Implies,
                    admin,
                    busy: busy(),
                    onadd: add,
                    onremove: remove,
                }
                section { class: "card",
                    h2 { class: "card-title", {t!("tags-implied-by")} }
                    p { class: "muted hint", {t!("tags-implied-by-hint")} }
                    if d.implied_by.is_empty() {
                        p { class: "muted", {t!("tags-implied-by-none")} }
                    } else {
                        div { class: "chips",
                            for t in d.implied_by.iter() {
                                Link { key: "{t.id}", class: "chip", to: Route::TagPage { name: t.name.clone() }, "{t.name}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RelationCard(
    title: String,
    hint: String,
    empty: String,
    names: Vec<String>,
    linked: bool,
    relation: Relation,
    admin: bool,
    busy: bool,
    onadd: EventHandler<(Relation, String)>,
    onremove: EventHandler<(Relation, String)>,
) -> Element {
    let mut text = use_signal(String::new);
    let label = match relation {
        Relation::Alias => t!("tags-new-alias"),
        Relation::Implies => t!("tags-implied-tag"),
    };
    rsx! {
        section { class: "card",
            h2 { class: "card-title", "{title}" }
            p { class: "muted hint", "{hint}" }
            if names.is_empty() {
                p { class: "muted", "{empty}" }
            } else {
                div { class: "chips",
                    for n in names {
                        span { key: "{n}", class: "chip removable",
                            if linked {
                                Link { to: Route::TagPage { name: n.clone() }, "{n}" }
                            } else {
                                span { "{n}" }
                            }
                            if admin {
                                button {
                                    class: "chip-x",
                                    disabled: busy,
                                    title: t!("tags-remove"),
                                    aria_label: t!("tags-remove-named", name = n.clone()),
                                    onclick: {
                                        let n = n.clone();
                                        move |_| onremove.call((relation, n.clone()))
                                    },
                                    Ico { icon: Icon::Close, size: 18 }
                                }
                            }
                        }
                    }
                }
            }
            if admin {
                form {
                    class: "inline-add",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        let value = text().trim().to_owned();
                        if !value.is_empty() {
                            onadd.call((relation, value));
                            text.set(String::new());
                        }
                    },
                    input {
                        aria_label: "{label}",
                        placeholder: "{label}",
                        autocomplete: "off",
                        spellcheck: "false",
                        value: "{text}",
                        oninput: move |e| text.set(e.value()),
                    }
                    button { class: "btn tonal small", r#type: "submit", disabled: busy || text().trim().is_empty(),
                        Ico { icon: Icon::Add }
                        {t!("add")}
                    }
                }
            } else {
                p { class: "muted fine", {t!("tags-admins-only")} }
            }
        }
    }
}
