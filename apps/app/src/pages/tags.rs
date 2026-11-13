//! The tag list, the tag page (category, place in the tree, aliases and
//! implications) and a post's tags grouped for the viewer.

use dioxus::prelude::*;
use starbooru_client::ClientError;
use starbooru_core::{PostTag, Role, SECTIONS, Tag, TagCategory, TagDetail, TagOrder, TagUpdate};

use super::menu::{ActionMenu, MenuItem};
use super::shell::notify;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::gesture::{MenuAt, Press, Target, use_press};
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

/// The chip class that colours a tag by its category.
pub(super) fn category_class(category: TagCategory) -> &'static str {
    match category {
        TagCategory::General => "chip",
        TagCategory::Artist => "chip cat-artist",
        TagCategory::Character => "chip cat-character",
        TagCategory::Copyright => "chip cat-copyright",
        TagCategory::Meta => "chip cat-meta",
    }
}

/// A heading in the viewer's tag card.
#[derive(Clone, Debug, PartialEq)]
enum Group {
    Category(TagCategory),
    Section(String),
    /// General tags outside any tree.
    Other,
}

impl Group {
    /// Artist, copyright, character, the built-in sections in order, other
    /// sections by name, general tags without one, meta.
    fn rank(&self) -> (u8, usize, &str) {
        match self {
            Group::Category(TagCategory::Artist) => (0, 0, ""),
            Group::Category(TagCategory::Copyright) => (1, 0, ""),
            Group::Category(TagCategory::Character) => (2, 0, ""),
            Group::Section(name) => match SECTIONS.iter().position(|s| s == name) {
                Some(i) => (3, i, ""),
                None => (4, 0, name),
            },
            Group::Other | Group::Category(TagCategory::General) => (5, 0, ""),
            Group::Category(TagCategory::Meta) => (6, 0, ""),
        }
    }

    fn label(&self) -> String {
        match self {
            Group::Category(c) => category_label(*c),
            Group::Section(name) => section_label(name),
            Group::Other => t!("section-other"),
        }
    }
}

fn section_label(name: &str) -> String {
    match name {
        "people" => t!("section-people"),
        "hair" => t!("section-hair"),
        "eyes" => t!("section-eyes"),
        "skin" => t!("section-skin"),
        "body" => t!("section-body"),
        "expression" => t!("section-expression"),
        "pose" => t!("section-pose"),
        "clothing" => t!("section-clothing"),
        "setting" => t!("section-setting"),
        "style" => t!("section-style"),
        other => {
            // a section someone made: `hair_accessories` reads "Hair accessories"
            let words = other.replace('_', " ");
            let mut chars = words.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

/// A post's tags under their headings, each group sorted by name.
fn group_tags(tags: &[PostTag]) -> Vec<(Group, Vec<PostTag>)> {
    let mut groups: Vec<(Group, Vec<PostTag>)> = Vec::new();
    for tag in tags {
        let group = match (&tag.category, &tag.section) {
            (TagCategory::General, Some(section)) => Group::Section(section.clone()),
            (TagCategory::General, None) => Group::Other,
            (category, _) => Group::Category(*category),
        };
        match groups.iter_mut().find(|(g, _)| *g == group) {
            Some((_, list)) => list.push(tag.clone()),
            None => groups.push((group, vec![tag.clone()])),
        }
    }
    groups.sort_by(|a, b| a.0.rank().cmp(&b.0.rank()));
    for (_, list) in &mut groups {
        list.sort_by(|a, b| a.name.cmp(&b.name));
    }
    groups
}

/// The viewer's tag chips, grouped when the server sent the details.
#[component]
pub fn TagGroups(details: Vec<PostTag>, names: Vec<String>) -> Element {
    if details.is_empty() {
        return rsx! {
            div { class: "chips", "data-press": "tag",
                for tag in names {
                    Link { key: "{tag}", class: "chip", to: Route::Gallery { q: tag.clone() }, "{tag}" }
                }
            }
        };
    }
    let groups = group_tags(&details);
    // one heading saying "Other" says nothing
    let headed = !matches!(groups.as_slice(), [(Group::Other, _)]);
    rsx! {
        div { class: "tag-groups",
            for (group, tags) in groups {
                section { key: "{group.label()}", class: "tag-group",
                    if headed {
                        h3 { "{group.label()}" }
                    }
                    div { class: "chips", "data-press": "tag",
                        for tag in tags {
                            Link {
                                key: "{tag.name}",
                                class: category_class(tag.category),
                                to: Route::Gallery { q: tag.name.clone() },
                                "{tag.name}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The words of a search, a quoted phrase (`text:"two words"`) staying one.
fn words(q: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let mut quoted = false;
    for (i, c) in q.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if c.is_whitespace() && !quoted {
            if let Some(from) = start.take() {
                out.extend(q.get(from..i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(from) = start {
        out.extend(q.get(from..));
    }
    out
}

/// `q` with `name` left out, whether it was wanted or excluded, and then
/// `word` at the end.
fn with_word(q: &str, name: &str, word: &str) -> String {
    let excluded = format!("-{name}");
    let mut kept: Vec<&str> = words(q)
        .into_iter()
        .filter(|w| *w != name && *w != excluded)
        .collect();
    kept.push(word);
    kept.join(" ")
}

/// The search narrowed to posts that also have `name`.
fn include(q: &str, name: &str) -> String {
    with_word(q, name, name)
}

/// The search without posts that have `name`.
fn exclude(q: &str, name: &str) -> String {
    with_word(q, name, &format!("-{name}"))
}

/// Copies `text` and says so. Where the page may not use the clipboard (plain
/// http), the old way still works.
fn copy_name(text: String) {
    dioxus::core::spawn_forever(async move {
        let eval = document::eval(
            r#"
            const text = await dioxus.recv();
            try {
              await navigator.clipboard.writeText(text);
            } catch {
              const field = document.createElement("textarea");
              field.value = text;
              field.style.position = "fixed";
              field.style.opacity = "0";
              document.body.append(field);
              field.select();
              const done = document.execCommand("copy");
              field.remove();
              if (!done) throw new Error("no clipboard");
            }
            return true;
            "#,
        );
        if eval.send(text).is_ok() && eval.await.is_ok() {
            notify(t!("tag-copied"));
        }
    });
}

/// The menu a held or right-clicked tag opens, on whatever page shows tags.
/// `q` is the search the page belongs to, which a tag can narrow.
#[component]
pub fn TagPress(q: String) -> Element {
    let mut menu = use_signal(|| None::<(String, MenuAt)>);
    use_press(
        "tag",
        use_callback(move |press: Press| {
            if let Press::Menu(Target::Tag(name), at) = press {
                menu.set(Some((name, at)));
            }
        }),
    );
    let Some((name, at)) = menu() else {
        return rsx! {};
    };
    let nav = navigator();
    let close = move |_| menu.set(None);
    let mut go = move |route: Route| {
        menu.set(None);
        nav.push(route);
    };
    let searching = !q.trim().is_empty();
    let has = words(&q).contains(&name.as_str());
    let narrowed = include(&q, &name);
    let without = exclude(if searching { &q } else { "" }, &name);
    let copied = name.clone();
    rsx! {
        ActionMenu { at, title: name.clone(), onclose: close,
            MenuItem {
                icon: Icon::Search,
                label: t!("tag-menu-search"),
                onclick: {
                    let name = name.clone();
                    move |_| go(Route::Gallery { q: name.clone() })
                },
            }
            if searching && !has {
                MenuItem {
                    icon: Icon::Add,
                    label: t!("tag-menu-add"),
                    onclick: move |_| go(Route::Gallery { q: narrowed.clone() }),
                }
            }
            MenuItem {
                icon: Icon::Remove,
                label: t!("tag-menu-exclude"),
                onclick: move |_| go(Route::Gallery { q: without.clone() }),
            }
            MenuItem {
                icon: Icon::Label,
                label: t!("tag-menu-details"),
                onclick: {
                    let name = name.clone();
                    move |_| go(Route::TagPage { name: name.clone() })
                },
            }
            MenuItem {
                icon: Icon::Copy,
                label: t!("tag-menu-copy"),
                onclick: move |_| {
                    menu.set(None);
                    copy_name(copied.clone());
                },
            }
        }
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
                ul { class: "tag-list card", "data-press": "tag",
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
            TagPress { q: String::new() }
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
            let update = TagUpdate {
                category: Some(category),
                parent: None,
            };
            match client.update_tag(&tag.tag.name, &update).await {
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

    // "" takes the tag out from under its parent.
    let set_parent = move |parent: String| {
        let (Some(client), Some(tag)) = (store::client(), detail.peek().clone()) else {
            return;
        };
        busy.set(true);
        spawn(async move {
            let name = tag.tag.name;
            let update = TagUpdate {
                category: None,
                parent: Some(parent),
            };
            let result = match client.update_tag(&name, &update).await {
                Ok(_) => client.tag(&name).await,
                Err(e) => Err(e),
            };
            apply(result);
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
                TreeCard {
                    detail: d.clone(),
                    busy: busy(),
                    onparent: set_parent,
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

/// Where the tag sits: its parents, a field to move it, and what is under it.
#[component]
fn TreeCard(detail: TagDetail, busy: bool, onparent: EventHandler<String>) -> Element {
    let mut text = use_signal(String::new);
    let has_parent = !detail.ancestors.is_empty();
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("tags-tree")} }
            p { class: "muted hint", {t!("tags-tree-hint")} }
            if has_parent {
                nav { class: "tag-trail", aria_label: t!("tags-tree"),
                    for a in detail.ancestors.iter() {
                        Link { key: "{a.id}", class: "chip", to: Route::TagPage { name: a.name.clone() }, "{a.name}" }
                        span { class: "sep", aria_hidden: "true", "›" }
                    }
                    span { class: "here", aria_current: "page", "{detail.tag.name}" }
                    button { class: "btn text small", r#type: "button", disabled: busy, onclick: move |_| onparent.call(String::new()),
                        {t!("tags-move-top")}
                    }
                }
            } else {
                p { class: "muted", {t!("tags-tree-top")} }
            }
            form {
                class: "inline-add",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    let value = text().trim().to_owned();
                    if !value.is_empty() {
                        onparent.call(value);
                        text.set(String::new());
                    }
                },
                input {
                    aria_label: t!("tags-parent"),
                    placeholder: t!("tags-parent"),
                    autocomplete: "off",
                    spellcheck: "false",
                    value: "{text}",
                    oninput: move |e| text.set(e.value()),
                }
                button { class: "btn tonal small", r#type: "submit", disabled: busy || text().trim().is_empty(),
                    {t!("tags-move")}
                }
            }
            h3 { class: "sub-title", {t!("tags-children")} }
            if detail.children.is_empty() {
                p { class: "muted", {t!("tags-children-none")} }
            } else {
                div { class: "chips",
                    for c in detail.children.iter() {
                        Link { key: "{c.id}", class: category_class(c.category), to: Route::TagPage { name: c.name.clone() }, "{c.name}" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quoted_phrase_is_one_word() {
        assert_eq!(
            words("  blue  text:\"two words\" -red "),
            ["blue", "text:\"two words\"", "-red"]
        );
        assert!(words("").is_empty());
        // A quote that never closes runs to the end.
        assert_eq!(words("a \"b c"), ["a", "\"b c"]);
    }

    #[test]
    fn a_tag_narrows_or_excludes_the_search() {
        assert_eq!(include("", "sky"), "sky");
        assert_eq!(include("blue order:new", "sky"), "blue order:new sky");
        // Excluded before: now wanted.
        assert_eq!(include("blue -sky", "sky"), "blue sky");
        assert_eq!(exclude("blue sky", "sky"), "blue -sky");
        assert_eq!(exclude("", "sky"), "-sky");
        // Already excluded: once is enough.
        assert_eq!(exclude("-sky blue", "sky"), "blue -sky");
        // Inside a phrase, the word is the phrase's.
        assert_eq!(exclude("text:\"a sky\"", "sky"), "text:\"a sky\" -sky");
        // Only the whole word.
        assert_eq!(include("skyline", "sky"), "skyline sky");
    }

    fn tag(name: &str, category: TagCategory, section: Option<&str>) -> PostTag {
        PostTag {
            name: name.into(),
            category,
            section: section.map(str::to_owned),
        }
    }

    #[test]
    fn groups_follow_the_viewer_order() {
        let tags = [
            tag("highres", TagCategory::Meta, None),
            tag("smile", TagCategory::General, Some("expression")),
            tag("solo", TagCategory::General, Some("people")),
            tag("sunflower", TagCategory::General, None),
            tag("boots", TagCategory::General, Some("clothing")),
            tag("hat", TagCategory::General, Some("clothing")),
            tag("miku", TagCategory::Character, None),
            tag("someone", TagCategory::Artist, Some("people")),
            tag("ribbon_thing", TagCategory::General, Some("my_stuff")),
            tag("vocaloid", TagCategory::Copyright, None),
        ];
        let groups = group_tags(&tags);
        let order: Vec<(Group, Vec<&str>)> = groups
            .iter()
            .map(|(g, list)| (g.clone(), list.iter().map(|t| t.name.as_str()).collect()))
            .collect();
        assert_eq!(
            order,
            [
                (Group::Category(TagCategory::Artist), vec!["someone"]),
                (Group::Category(TagCategory::Copyright), vec!["vocaloid"]),
                (Group::Category(TagCategory::Character), vec!["miku"]),
                (Group::Section("people".into()), vec!["solo"]),
                (Group::Section("expression".into()), vec!["smile"]),
                (Group::Section("clothing".into()), vec!["boots", "hat"]),
                (Group::Section("my_stuff".into()), vec!["ribbon_thing"]),
                (Group::Other, vec!["sunflower"]),
                (Group::Category(TagCategory::Meta), vec!["highres"]),
            ]
        );
    }

    #[test]
    fn own_sections_get_readable_names() {
        assert_eq!(section_label("hair_accessories"), "Hair accessories");
        assert_eq!(section_label(""), "");
    }
}
