//! App frame: floating capsules for the brand and the search, and a slider for
//! the navigation (top on wide screens, a rail in between, bottom on phones).
//! Shows the sign-in screen when there is no session.

use std::cell::Cell;
use std::cmp::Ordering;
use std::mem::discriminant;
use std::rc::Rc;

use crate::link::Link;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use starbooru_core::{RemoteTag, RemoteTagQuery, Tag, TagCategory};

use super::boorus;
use super::login::Login;
use super::search_demo::{SearchDemo, plays_on};
use crate::Route;
use crate::activity::{self, Activity};
use crate::brand::{Mark, Wordmark};
use crate::icons::{Ico, Icon};
use crate::store::{self, SESSION};
use crate::util::sleep_ms;

#[component]
pub fn Shell() -> Element {
    if SESSION.read().is_none() {
        return rsx! { Login {} };
    }
    let route = use_route::<Route>();
    #[cfg(target_os = "android")]
    crate::back::watch(&route);
    // Opening the app carries on from the last search.
    use_hook(|| {
        let last = store::last_search();
        if !last.is_empty() && matches!(&route, Route::Gallery { q } if q.is_empty()) {
            navigator().replace(Route::Gallery { q: last });
        }
    });
    rsx! {
        header { class: "topbar",
            Link { class: "brand glass float", aria_label: "StarBooru", to: Route::Gallery { q: String::new() },
                Mark { height: 30, compact: true }
                Wordmark { height: 19 }
            }
            SearchBox { route: route.clone() }
            NavSlider { place: "nav-top", route: route.clone() }
        }
        main { class: "content", Outlet::<Route> {} }
        NavSlider { place: "nav-bottom", route }
        Snackbar {}
        Activity {}
    }
}

/// What the snackbar shows, numbered so a new one restarts its timer.
#[derive(Clone)]
struct Notice {
    n: u64,
    message: String,
    action: Option<NoticeAction>,
    ms: u32,
}

/// A button on a notice, such as Undo. It runs in the snackbar, which stays
/// when the page changes.
#[derive(Clone)]
struct NoticeAction {
    label: String,
    run: Rc<dyn Fn()>,
}

static NOTICE: GlobalSignal<Option<Notice>> = Signal::global(|| None);

/// Shows a short message at the bottom of the screen for a few seconds,
/// for things that finish after the click (a download, say).
pub fn notify(message: String) {
    show_notice(message, None, NOTICE_MS);
}

/// A notice with a button that takes back what just happened. It stays a
/// little longer than others, and a line under it runs out with the time.
pub fn notify_undo(message: String, label: String, run: impl Fn() + 'static) {
    let action = NoticeAction {
        label,
        run: Rc::new(run),
    };
    show_notice(message, Some(action), UNDO_MS);
}

fn show_notice(message: String, action: Option<NoticeAction>, ms: u32) {
    let n = NOTICE.peek().as_ref().map_or(0, |m| m.n.wrapping_add(1));
    *NOTICE.write() = Some(Notice {
        n,
        message,
        action,
        ms,
    });
}

/// How long a notice stays, in milliseconds.
const NOTICE_MS: u32 = 6000;
/// How long an undo is offered.
const UNDO_MS: u32 = 8000;

#[component]
fn Snackbar() -> Element {
    use_effect(|| {
        let Some((n, ms)) = NOTICE.read().as_ref().map(|m| (m.n, m.ms)) else {
            return;
        };
        spawn(async move {
            // A timer that works the same in the browser and the webviews.
            let _ = document::eval(&format!(
                "await new Promise((r) => setTimeout(r, {ms})); return true;"
            ))
            .join::<bool>()
            .await;
            if NOTICE.peek().as_ref().is_some_and(|m| m.n == n) {
                *NOTICE.write() = None;
            }
        });
    });
    let notice = NOTICE.read().clone();
    let timed = notice.as_ref().is_some_and(|m| m.action.is_some());
    rsx! {
        div { class: "snackbar-slot", role: "status", aria_live: "polite",
            if let Some(Notice { n, message, action, ms }) = notice {
                div { key: "{n}", class: if timed { "snackbar timed" } else { "snackbar" },
                    style: "--notice-ms: {ms}ms",
                    span { class: "break", "{message}" }
                    if let Some(NoticeAction { label, run }) = action {
                        button {
                            class: "btn text snackbar-action",
                            onclick: move |_| {
                                *NOTICE.write() = None;
                                run();
                            },
                            "{label}"
                        }
                    }
                    button {
                        class: "icon-btn",
                        aria_label: t!("notice-close"),
                        title: t!("notice-close"),
                        onclick: move |_| *NOTICE.write() = None,
                        Ico { icon: Icon::Close }
                    }
                }
            }
        }
    }
}

/// The tab a page is under, so the slider always says where you are. The
/// booru browser is the other side of Explore (`explore::Sides`).
fn tab_of(route: &Route) -> Route {
    match route {
        Route::PostView { .. } => Route::Gallery { q: String::new() },
        Route::TagPage { .. } => Route::Tags {},
        Route::Boorus { .. } | Route::BooruPost { .. } => Route::Explore { q: String::new() },
        Route::Trash {} | Route::Duplicates {} | Route::Help {} | Route::Admin {} => {
            Route::Settings {}
        }
        other => other.clone(),
    }
}

thread_local! {
    /// Whether Explore was last open on the boorus.
    static ON_BOORUS: Cell<bool> = const { Cell::new(false) };
}

/// Whether Explore is on the boorus after this page, given where it was.
/// Pages under other tabs leave it as it is.
fn on_boorus(route: &Route, was: bool) -> bool {
    match route {
        Route::Boorus { .. } | Route::BooruPost { .. } => true,
        Route::Explore { .. } => false,
        _ => was,
    }
}

/// The navigation's items in order, five at most (`docs/design-ui-ux`): a
/// new page goes under one of these. `gallery` and `explore` are where those
/// two go back to.
fn tabs(gallery: Route, explore: Route, can_upload: bool) -> Vec<(Route, Icon, String)> {
    [
        (gallery, Icon::Gallery, t!("nav-gallery")),
        (explore, Icon::Explore, t!("nav-explore")),
        (Route::Tags {}, Icon::Label, t!("nav-tags")),
        (Route::Upload {}, Icon::Upload, t!("nav-upload")),
        (Route::Settings {}, Icon::Tune, t!("nav-settings-tab")),
    ]
    .into_iter()
    .filter(|(to, ..)| !matches!(to, Route::Upload {}) || can_upload)
    .collect()
}

/// Which of the navigation's items the page is under. An item is told by its
/// own tab, as Explore's may lead to the boorus.
fn tab_at(items: &[(Route, Icon, String)], route: &Route) -> Option<usize> {
    let tab = discriminant(&tab_of(route));
    items
        .iter()
        .position(|(to, ..)| discriminant(&tab_of(to)) == tab)
}

/// `place` is the class that says where this copy of the navigation shows.
#[component]
fn NavSlider(place: &'static str, route: Route) -> Element {
    let boorus = ON_BOORUS.with(|side| {
        side.set(on_boorus(&route, side.get()));
        side.get()
    });
    // Each goes back to where it was left: the gallery to its last search,
    // Explore to the side last open, and the boorus to their last search.
    let explore = if boorus {
        boorus::last_route()
    } else {
        Route::Explore { q: String::new() }
    };
    let gallery = Route::Gallery {
        q: store::last_search(),
    };
    let items = tabs(gallery, explore, store::can_upload());
    let here = tab_at(&items, &route);
    // Where the thumb stands and the way it last went, which says which of its
    // edges leads. With no tab selected it keeps its place, out of sight.
    let last = use_hook(|| Rc::new(Cell::new((here.unwrap_or(0), "next"))));
    let (was, way) = last.get();
    let at = here.unwrap_or(was);
    let way = match at.cmp(&was) {
        Ordering::Greater => "next",
        Ordering::Less => "back",
        Ordering::Equal => way,
    };
    last.set((at, way));
    let cells = items.len();
    // Uploads and imports still going, shown on the Upload item.
    let pending = activity::pending();
    let note = t!("activity-pending", count = pending);
    rsx! {
        nav {
            class: "{place} slider glass float",
            aria_label: t!("nav-main"),
            style: "--n: {cells}; --i: {at}",
            "data-dir": way,
            span { class: if here.is_some() { "slider-thumb" } else { "slider-thumb off" },
                span { class: "slider-clip",
                    span { class: "slider-unclip",
                        span { class: "slider-fill" }
                    }
                }
            }
            for (i, (to, icon, label)) in items.into_iter().enumerate() {
                NavLink {
                    key: "{label}",
                    active: here == Some(i),
                    badge: if matches!(to, Route::Upload {}) { pending } else { 0 },
                    note: note.clone(),
                    to,
                    icon,
                    label,
                }
            }
        }
    }
}

/// `badge` counts work still going on that page; `note` says so in words.
#[component]
fn NavLink(
    to: Route,
    icon: Icon,
    label: String,
    active: bool,
    badge: usize,
    note: String,
) -> Element {
    // The upload sheet comes out of this tab (`.nav-upload`).
    let upload = if matches!(to, Route::Upload {}) {
        " nav-upload"
    } else {
        ""
    };
    let on = if active { " active" } else { "" };
    rsx! {
        Link {
            class: "nav-item{upload}{on}",
            title: if badge > 0 { note.clone() } else { String::new() },
            aria_current: if active { "page" } else { "false" },
            to,
            span { class: "nav-icon",
                // Its own box for the pop, apart from the press on `nav-icon`.
                span { class: "nav-pop", Ico { icon } }
                if badge > 0 {
                    span { class: "nav-badge", aria_hidden: "true", "{badge.min(99)}" }
                }
            }
            span { class: "nav-label", "{label}" }
            if badge > 0 {
                span { class: "visually-hidden", "{note}" }
            }
        }
    }
}

/// The query the search box shows for a route: the one in the route, else the
/// last search, which is where the Gallery tab goes.
fn route_query(route: &Route, last: &str) -> String {
    match route {
        Route::Gallery { q } | Route::PostView { q, .. } => q.clone(),
        // The search as it was typed; the address carries it encoded.
        Route::Boorus { q, .. } | Route::BooruPost { q, .. } => boorus::from_address(q),
        _ => last.to_owned(),
    }
}

/// What the search box searches: the collection, or, on the booru browser's
/// pages, the sites chosen there (as the address names them).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Scope {
    Collection,
    Boorus(String),
}

fn scope_of(route: &Route) -> Scope {
    match route {
        Route::Boorus { s, .. } | Route::BooruPost { s, .. } => Scope::Boorus(s.clone()),
        _ => Scope::Collection,
    }
}

/// Where a search typed in the box goes.
fn search_route(scope: &Scope, q: &str) -> Route {
    match scope {
        Scope::Collection => Route::Gallery {
            q: q.trim().to_owned(),
        },
        Scope::Boorus(s) => boorus::search_route(q.trim(), s),
    }
}

/// The word being typed, split into what leads it (`-`, `{`) and the tag.
fn split_word(word: &str) -> (&str, &str) {
    let tag = word.trim_start_matches(['-', '{']);
    (word.strip_suffix(tag).unwrap_or_default(), tag)
}

/// The tag being typed at the end of the search box, if there is one to
/// complete. A finished word, a `key:value` term and a closed group have none.
fn typed_tag(value: &str) -> Option<&str> {
    if value.ends_with(' ') {
        return None;
    }
    let (_, tag) = split_word(value.split_whitespace().last()?);
    (!tag.is_empty() && !tag.contains(':') && !tag.ends_with('}')).then_some(tag)
}

/// How long the typing has to pause before the server is asked for
/// suggestions. A word typed in one go then costs one request, where every
/// letter used to cost one.
const TYPING_PAUSE_MS: u32 = 120;
/// The same for another site's tags, which that site is asked for: longer, as
/// each site only takes so many requests.
const REMOTE_PAUSE_MS: u32 = 250;
/// How many answers are kept, and for how long: tags and their counts change.
const KEPT_ANSWERS: usize = 48;
const KEPT_SECONDS: i64 = 60;

/// Recent suggestions by what was typed, so taking a letter back or typing a
/// word again shows them at once and asks nothing.
struct Answers<T> {
    /// Whose they are: another account sees other tags, and so does another
    /// site.
    owner: String,
    /// The latest last.
    answers: Vec<(String, DateTime<Utc>, Vec<T>)>,
}

impl<T> Default for Answers<T> {
    fn default() -> Self {
        Self {
            owner: String::new(),
            answers: Vec::new(),
        }
    }
}

impl<T: Clone> Answers<T> {
    fn get(&self, owner: &str, typed: &str, now: DateTime<Utc>) -> Option<Vec<T>> {
        self.answers
            .iter()
            .find(|(was, at, _)| {
                self.owner == owner && was == typed && (now - *at).num_seconds() < KEPT_SECONDS
            })
            .map(|(.., tags)| tags.clone())
    }

    fn put(&mut self, owner: &str, typed: String, tags: Vec<T>, now: DateTime<Utc>) {
        if self.owner != owner {
            self.owner = owner.to_owned();
            self.answers.clear();
        }
        self.answers.retain(|(was, ..)| *was != typed);
        self.answers.push((typed, now, tags));
        let extra = self.answers.len().saturating_sub(KEPT_ANSWERS);
        self.answers.drain(..extra);
    }
}

/// The collection's own tags.
type Suggested = Answers<Tag>;

thread_local! {
    static SUGGESTED: std::cell::RefCell<Suggested> = std::cell::RefCell::default();
    static REMOTE_SUGGESTED: std::cell::RefCell<Answers<RemoteTag>> = std::cell::RefCell::default();
}

/// The collection's tags starting with `typed`, for a box being typed in:
/// from the answers kept, or asked for once the typing pauses. The next
/// letter drops the future that waits, so a word typed in one go asks once.
pub(crate) async fn local_tags(typed: String) -> Vec<Tag> {
    let owner = store::owner();
    if let Some(known) = SUGGESTED.with_borrow(|s| s.get(&owner, &typed, Utc::now())) {
        return known;
    }
    sleep_ms(TYPING_PAUSE_MS).await;
    let Some(client) = store::client() else {
        return Vec::new();
    };
    let Ok(tags) = client.tags(&typed, 8).await else {
        return Vec::new();
    };
    SUGGESTED.with_borrow_mut(|s| s.put(&owner, typed, tags.clone(), Utc::now()));
    tags
}

/// A tag offered under the search box, the collection's or another site's.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Hint {
    name: String,
    /// The class that colours it by category; none for the collection's.
    class: &'static str,
    /// How many posts carry it, when that is known.
    count: Option<String>,
}

impl From<Tag> for Hint {
    fn from(tag: Tag) -> Self {
        Self {
            name: tag.name,
            class: "",
            count: Some(tag.post_count.to_string()),
        }
    }
}

impl From<RemoteTag> for Hint {
    fn from(tag: RemoteTag) -> Self {
        Self {
            name: tag.name,
            class: match tag.category {
                TagCategory::General => "",
                TagCategory::Artist => "cat-artist",
                TagCategory::Character => "cat-character",
                TagCategory::Copyright => "cat-copyright",
                TagCategory::Meta => "cat-meta",
            },
            count: tag.count.map(short_count),
        }
    }
}

fn hints<T: Into<Hint>>(tags: Vec<T>) -> Vec<Hint> {
    tags.into_iter().map(Into::into).collect()
}

/// A count in few characters: the big sites have tags on millions of posts.
/// `1234` → `1.2k`, `4217021` → `4.2M`.
fn short_count(n: u64) -> String {
    let short = |unit: u64, letter: &str| {
        let (whole, tenth) = (n / unit, (n % unit).saturating_mul(10) / unit);
        if whole >= 100 {
            format!("{whole}{letter}")
        } else {
            format!("{whole}.{tenth}{letter}")
        }
    };
    match n {
        ..1_000 => n.to_string(),
        1_000..1_000_000 => short(1_000, "k"),
        _ => short(1_000_000, "M"),
    }
}

/// Tag search with autocomplete on the word being typed.
#[component]
fn SearchBox(route: Route) -> Element {
    let mut text = use_signal(|| route_query(&route, &store::last_search()));
    let mut scope = use_signal(|| scope_of(&route));
    let mut focused = use_signal(|| false);
    // The highlighted suggestion, moved with the arrow keys.
    let mut active = use_signal(|| None::<usize>);
    // Every navigation resets the box, so a half-typed query doesn't linger.
    use_effect(use_reactive!(|route| {
        text.set(route_query(&route, &store::last_search()));
        let now = scope_of(&route);
        if *scope.peek() != now {
            scope.set(now);
        }
    }));

    let suggestions = use_resource(move || async move {
        let value = text();
        let scope = scope();
        let Some(typed) = typed_tag(&value).map(str::to_owned) else {
            return Vec::<Hint>::new();
        };
        let owner = store::owner();
        let Scope::Boorus(sites) = scope else {
            return hints(local_tags(typed).await);
        };
        // The booru browser: the first of its sites says how its tags go on.
        let Some(site) = boorus::completing_site(&sites) else {
            return Vec::new();
        };
        let whose = format!("{owner}|{}|{}", site.api.as_str(), site.base);
        if let Some(known) = REMOTE_SUGGESTED.with_borrow(|s| s.get(&whose, &typed, Utc::now())) {
            return hints(known);
        }
        sleep_ms(REMOTE_PAUSE_MS).await;
        let Some(client) = store::client() else {
            return Vec::new();
        };
        let query = RemoteTagQuery {
            api: site.api,
            base: site.base,
            term: typed.clone(),
        };
        let Ok(tags) = client.remote_tags(&query).await else {
            return Vec::new();
        };
        REMOTE_SUGGESTED.with_borrow_mut(|s| s.put(&whose, typed, tags.clone(), Utc::now()));
        hints(tags)
    });

    let mut search = move |q: String| {
        focused.set(false);
        let to = search_route(&scope.peek(), &q);
        navigator().push(to);
    };

    let mut complete = move |tag: String| {
        let value = text();
        let mut words: Vec<&str> = value.split_whitespace().collect();
        let lead = words.pop().map_or("", |w| split_word(w).0);
        let mut next = words.join(" ");
        if !next.is_empty() {
            next.push(' ');
        }
        next.push_str(lead);
        next.push_str(&tag);
        next.push(' ');
        text.set(next);
    };

    let list = suggestions.read().clone().unwrap_or_default();
    let open = focused() && !list.is_empty();
    let count = list.len();
    let names: Vec<String> = list.iter().map(|t| t.name.clone()).collect();
    let selected = active().filter(|i| *i < count);
    let remote = matches!(*scope.read(), Scope::Boorus(_));
    let label = if remote {
        t!("boorus-search")
    } else {
        t!("search-label")
    };
    rsx! {
        form {
            class: "search glass float",
            role: "search",
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                search(text());
            },
            // Phones: the crown takes the search icon's place, and the way home with it.
            Link { class: "search-home", aria_label: "StarBooru", to: Route::Gallery { q: String::new() },
                Mark { height: 28, compact: true }
            }
            Ico { icon: Icon::Search }
            div { class: "search-field",
            input {
                r#type: "search",
                placeholder: if remote { t!("boorus-search") } else { t!("search-placeholder") },
                aria_label: label,
                autocomplete: "off",
                spellcheck: "false",
                value: "{text}",
                role: "combobox",
                aria_autocomplete: "list",
                aria_controls: "search-suggestions",
                aria_expanded: if open { "true" } else { "false" },
                aria_activedescendant: selected.map(|i| format!("suggestion-{i}")).unwrap_or_default(),
                oninput: move |e| {
                    text.set(e.value());
                    active.set(None);
                    focused.set(true);
                },
                onfocus: move |_| focused.set(true),
                onblur: move |_| {
                    focused.set(false);
                    active.set(None);
                },
                onkeydown: move |e: KeyboardEvent| {
                    if !open {
                        return;
                    }
                    match e.key() {
                        Key::ArrowDown => {
                            e.prevent_default();
                            active.set(Some(selected.map_or(0, |i| (i + 1) % count)));
                        }
                        Key::ArrowUp => {
                            e.prevent_default();
                            active.set(Some(selected.map_or(count - 1, |i| (i + count - 1) % count)));
                        }
                        Key::Enter | Key::Tab => {
                            if let Some(name) = selected.and_then(|i| names.get(i)) {
                                e.prevent_default();
                                complete(name.clone());
                                active.set(None);
                            }
                        }
                        Key::Escape => {
                            e.prevent_default();
                            focused.set(false);
                            active.set(None);
                        }
                        _ => {}
                    }
                },
            }
            if text.read().is_empty() && plays_on(&route) {
                SearchDemo {}
            }
            }
            if open {
                ul { class: "suggestions", role: "listbox", id: "search-suggestions",
                    for (i, tag) in list.into_iter().enumerate() {
                        li {
                            key: "{tag.name}",
                            id: "suggestion-{i}",
                            role: "option",
                            class: if selected == Some(i) { "active" } else { "" },
                            aria_selected: if selected == Some(i) { "true" } else { "false" },
                            onmouseenter: move |_| active.set(Some(i)),
                            // mousedown fires before the input's blur
                            onmousedown: move |e| {
                                e.prevent_default();
                                complete(tag.name.clone());
                            },
                            span { class: "tag-name {tag.class}", "{tag.name}" }
                            if let Some(count) = &tag.count {
                                span { class: "count", "{count}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Answers, Hint, KEPT_ANSWERS, KEPT_SECONDS, REMOTE_PAUSE_MS, Scope, Suggested,
        TYPING_PAUSE_MS, hints, on_boorus, route_query, scope_of, search_route, short_count,
        split_word, tab_at, tab_of, tabs, typed_tag,
    };
    use crate::Route;
    use chrono::{Duration, Utc};
    use starbooru_core::{RemoteTag, Tag, TagCategory};
    use std::mem::discriminant;

    fn tag(name: &str) -> Tag {
        serde_json::from_value(serde_json::json!({
            "id": 1, "name": name, "category": "general", "post_count": 3,
        }))
        .unwrap()
    }

    #[test]
    fn only_a_tag_still_being_typed_is_completed() {
        assert_eq!(typed_tag("blue_sk"), Some("blue_sk"));
        assert_eq!(typed_tag("night -clo"), Some("clo"));
        assert_eq!(typed_tag("{girl boo"), Some("boo"));
        assert_eq!(typed_tag("-{bo"), Some("bo"));
        for done in [
            "",
            "   ",
            "night ",
            "rating:s",
            "order:sc",
            "{girl boots}",
            "-",
            "{",
        ] {
            assert_eq!(typed_tag(done), None, "{done:?}");
        }
    }

    #[test]
    fn an_answer_is_kept_for_a_minute_and_for_its_account() {
        let now = Utc::now();
        let mut kept = Suggested::default();
        kept.put("a|1", "blu".into(), vec![tag("blue_sky")], now);
        let names = |found: Option<Vec<Tag>>| {
            found.map(|t| t.into_iter().map(|t| t.name).collect::<Vec<_>>())
        };
        assert_eq!(
            names(kept.get("a|1", "blu", now)),
            Some(vec!["blue_sky".to_owned()])
        );
        assert_eq!(names(kept.get("a|1", "blue", now)), None);
        assert_eq!(names(kept.get("b|2", "blu", now)), None);
        let later = now + Duration::seconds(KEPT_SECONDS);
        assert_eq!(names(kept.get("a|1", "blu", later)), None);
        // Another account's first answer empties what the last one saw.
        kept.put("b|2", "x".into(), Vec::new(), now);
        assert_eq!(names(kept.get("a|1", "blu", now)), None);
        assert_eq!(names(kept.get("b|2", "x", now)), Some(Vec::new()));
    }

    #[test]
    fn old_answers_make_room() {
        let now = Utc::now();
        let mut kept = Suggested::default();
        for i in 0..KEPT_ANSWERS + 10 {
            kept.put("a|1", format!("t{i}"), Vec::new(), now);
            // Asked again: one entry, not two.
            kept.put("a|1", format!("t{i}"), Vec::new(), now);
        }
        assert_eq!(kept.answers.len(), KEPT_ANSWERS);
        assert!(kept.get("a|1", "t0", now).is_none());
        assert!(
            kept.get("a|1", &format!("t{}", KEPT_ANSWERS + 9), now)
                .is_some()
        );
    }

    #[test]
    fn the_rail_and_its_thumb_round_the_same_sides() {
        // Regression: cells shorter than the thumb is wide made a thumb flat on top
        // and bottom, in a rail flat on left and right.
        let css = include_str!("../../assets/main.css");
        assert!(css.contains("min-height: var(--rail-w);"));
        assert!(css.contains("min-height: calc(var(--rail-w) - 2 * var(--pad));"));
        assert!(css.contains("border-radius: calc(var(--rail-w) / 2 - var(--pad));"));
    }

    #[test]
    fn a_page_under_a_tab_keeps_that_tab_selected() {
        let settings = discriminant(&Route::Settings {});
        for page in [
            Route::Trash {},
            Route::Duplicates {},
            Route::Help {},
            Route::Admin {},
        ] {
            assert_eq!(discriminant(&tab_of(&page)), settings);
        }
        let post = Route::PostView {
            id: 1,
            q: String::new(),
        };
        assert_eq!(
            discriminant(&tab_of(&post)),
            discriminant(&Route::Gallery { q: String::new() })
        );
        assert_eq!(tab_of(&Route::TagPage { name: "a".into() }), Route::Tags {});
        // The booru browser and its posts are under Explore.
        let explore = discriminant(&Route::Explore { q: String::new() });
        assert_eq!(discriminant(&tab_of(&booru_search())), explore);
        assert_eq!(discriminant(&tab_of(&booru_post())), explore);
    }

    fn booru_search() -> Route {
        Route::Boorus {
            q: "blue_sky".into(),
            s: "danbooru".into(),
        }
    }

    fn booru_post() -> Route {
        Route::BooruPost {
            u: "https://d.org/posts/1".into(),
            q: "blue_sky".into(),
            s: "danbooru".into(),
        }
    }

    #[test]
    fn the_navigation_holds_five_items_at_most() {
        // Regression: the booru browser came as a sixth, which the design
        // rules don't allow. Every page is still under one of the five.
        const MAX_TABS: usize = 5;
        let gallery = Route::Gallery { q: String::new() };
        let explore = Route::Explore { q: String::new() };
        let all = tabs(gallery.clone(), explore.clone(), true);
        assert_eq!(all.len(), MAX_TABS);
        assert_eq!(tabs(gallery, explore, false).len(), MAX_TABS - 1);
        for (page, at) in [
            (booru_search(), 1),
            (booru_post(), 1),
            (Route::Trash {}, 4),
            (Route::TagPage { name: "a".into() }, 2),
            (Route::Upload {}, 3),
        ] {
            assert_eq!(tab_at(&all, &page), Some(at), "{page:?}");
        }
        // Regression: with Explore leading back to the boorus, its item went
        // unlit on their pages.
        let back = tabs(Route::Gallery { q: String::new() }, booru_search(), true);
        assert_eq!(tab_at(&back, &booru_post()), Some(1));
        assert_eq!(tab_at(&back, &Route::Explore { q: String::new() }), Some(1));
    }

    #[test]
    fn explore_goes_back_to_the_side_last_open() {
        // On the boorus, and on a post there.
        assert!(on_boorus(&booru_search(), false));
        assert!(on_boorus(&booru_post(), false));
        // Other tabs in between leave it there.
        assert!(on_boorus(&Route::Tags {}, true));
        assert!(on_boorus(&Route::Gallery { q: "cat".into() }, true));
        assert!(!on_boorus(&Route::Settings {}, false));
        // The collection's side puts it back.
        assert!(!on_boorus(&Route::Explore { q: String::new() }, true));
    }

    #[test]
    fn the_box_searches_the_boorus_on_their_pages() {
        let search = Route::Boorus {
            q: "blue_sky".into(),
            s: "danbooru,gelbooru".into(),
        };
        let post = Route::BooruPost {
            u: "https://d.org/posts/1".into(),
            q: "blue_sky".into(),
            s: "e926".into(),
        };
        // The box shows the browser's own search here.
        assert_eq!(route_query(&search, "dog"), "blue_sky");
        assert_eq!(route_query(&post, "dog"), "blue_sky");
        assert_eq!(scope_of(&search), Scope::Boorus("danbooru,gelbooru".into()));
        assert_eq!(scope_of(&post), Scope::Boorus("e926".into()));
        // A new search goes to the same sites.
        assert_eq!(
            search_route(&scope_of(&search), " night_sky "),
            Route::Boorus {
                q: "night_sky".into(),
                s: "danbooru,gelbooru".into()
            }
        );
        // Everywhere else it is the gallery's, as before.
        for other in [
            Route::Gallery { q: "cat".into() },
            Route::Settings {},
            Route::Upload {},
        ] {
            assert_eq!(scope_of(&other), Scope::Collection);
        }
        assert_eq!(
            search_route(&Scope::Collection, " cat "),
            Route::Gallery { q: "cat".into() }
        );
        // A tag with a sign the router would misread goes into the address
        // in a form it leaves alone, and the box shows it as typed.
        let odd = search_route(&scope_of(&search), "panty_&_stocking");
        assert_eq!(
            odd,
            Route::Boorus {
                q: "panty_~26_stocking".into(),
                s: "danbooru,gelbooru".into()
            }
        );
        assert_eq!(route_query(&odd, ""), "panty_&_stocking");
    }

    #[test]
    fn another_sites_tags_show_their_category_and_a_short_count() {
        let remote = |name: &str, category, count| RemoteTag {
            name: name.into(),
            category,
            count,
        };
        let list = hints(vec![
            remote("blush", TagCategory::General, Some(4_217_021)),
            remote("blue_archive", TagCategory::Copyright, Some(9)),
            remote("someone", TagCategory::Artist, None),
        ]);
        let shown: Vec<(&str, &str, Option<&str>)> = list
            .iter()
            .map(|h| (h.name.as_str(), h.class, h.count.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [
                ("blush", "", Some("4.2M")),
                ("blue_archive", "cat-copyright", Some("9")),
                ("someone", "cat-artist", None),
            ]
        );
        // The collection's own are as they were: no colour, the count as it is.
        assert_eq!(
            Hint::from(tag("blue_sky")),
            Hint {
                name: "blue_sky".into(),
                class: "",
                count: Some("3".into())
            }
        );
        for (n, short) in [
            (0, "0"),
            (999, "999"),
            (1_000, "1.0k"),
            (1_234, "1.2k"),
            (99_949, "99.9k"),
            (123_456, "123k"),
            (999_999, "999k"),
            (1_000_000, "1.0M"),
            (4_217_021, "4.2M"),
            (u64::MAX, "18446744073709M"),
        ] {
            assert_eq!(short_count(n), short, "{n}");
        }
        // The colours are in the stylesheet, for the list under the box.
        let css = include_str!("../../assets/main.css");
        for class in ["cat-artist", "cat-character", "cat-copyright", "cat-meta"] {
            assert!(css.contains(&format!(
                ".suggestions .{class} {{ color: var(--{class}); }}"
            )));
        }
    }

    #[test]
    fn another_sites_answers_are_kept_apart_and_asked_for_less_often() {
        let now = Utc::now();
        let mut kept: Answers<RemoteTag> = Answers::default();
        let blush = RemoteTag {
            name: "blush".into(),
            category: TagCategory::General,
            count: Some(1),
        };
        kept.put("a|1|danbooru|https://d.org", "blu".into(), vec![blush], now);
        let found = kept.get("a|1|danbooru|https://d.org", "blu", now);
        assert_eq!(found.map(|tags| tags.len()), Some(1));
        // Another site has other tags.
        assert!(kept.get("a|1|gelbooru|https://g.org", "blu", now).is_none());
        assert!(REMOTE_PAUSE_MS > TYPING_PAUSE_MS);
        assert_eq!(REMOTE_PAUSE_MS, 250);
    }

    #[test]
    fn the_search_stays_on_the_middle_of_the_window() {
        // Equal columns either side: content can't push it off centre.
        let css = include_str!("../../assets/main.css");
        assert!(css.contains("grid-template-columns: var(--side) minmax(0, 1fr) var(--side)"));
    }

    #[test]
    fn the_bars_float() {
        let css = include_str!("../../assets/main.css");
        let (_, topbar) = css.split_once(".topbar {").unwrap();
        let (topbar, _) = topbar.split_once('}').unwrap();
        assert!(topbar.contains("position: fixed"));
        assert!(topbar.contains("pointer-events: none"));
        assert!(!topbar.contains("border-bottom"));
    }

    #[test]
    fn the_box_shows_the_routes_search_or_else_the_last() {
        let gallery = Route::Gallery { q: "cat".into() };
        assert_eq!(route_query(&gallery, "dog"), "cat");
        let everything = Route::Gallery { q: String::new() };
        assert_eq!(route_query(&everything, "dog"), "");
        let post = Route::PostView {
            id: 1,
            q: "cat".into(),
        };
        assert_eq!(route_query(&post, "dog"), "cat");
        assert_eq!(route_query(&Route::Settings {}, "dog"), "dog");
    }

    #[test]
    fn words_split_from_what_leads_them() {
        assert_eq!(split_word("boots"), ("", "boots"));
        assert_eq!(split_word("-boots"), ("-", "boots"));
        assert_eq!(split_word("{boots"), ("{", "boots"));
        assert_eq!(split_word("-{boots"), ("-{", "boots"));
        assert_eq!(split_word("-{"), ("-{", ""));
        assert_eq!(split_word(""), ("", ""));
    }
}
