//! One post on another booru: the picture large, its tags by category, what
//! its site says about it, and the button that saves it here.
//!
//! It shows what the search brought (`boorus::around`). A post opened from a
//! link, one whose site showed no file, and one from a Gelbooru-kind site
//! that came without categories is asked for by itself, once.

use std::collections::HashMap;
use std::rc::Rc;

use crate::link::Link;
use dioxus::prelude::*;
use starbooru_core::{BooruApi, RemotePost, TagCategory, guarded_by};

use super::boorus::{self, Status, Twin};
use super::tags::{category_class, category_label};
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::activity::{self, JOBS};
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{human_size, rating_label, short_date};

/// What a site said about a post asked for by itself, or why it said nothing.
type Answer = Result<Rc<RemotePost>, String>;

/// The answers so far, by whose they are and the post's page, so no post is
/// asked for twice while the app runs.
static LOOKED: GlobalSignal<HashMap<(String, String), Answer>> = Signal::global(HashMap::new);
/// Most answers kept; past it, they all go and are asked for again.
const MAX_LOOKED: usize = 200;

/// Arrow keys and Escape for the page. Typing in a field, a playing video's
/// own keys and anything open over the page are left alone.
const KEYS: &str = r#"
if (window.__sbBooruKeys) removeEventListener("keydown", window.__sbBooruKeys);
window.__sbBooruKeys = (e) => {
  if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
  const t = e.target;
  if (t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT|VIDEO)$/.test(t.tagName))) return;
  if (document.querySelector(".sheet-scrim, .select-backdrop, .press-backdrop")) return;
  if (["ArrowLeft", "ArrowRight", "Escape"].includes(e.key)) dioxus.send(e.key);
};
addEventListener("keydown", window.__sbBooruKeys);
"#;
/// Stops listening for those keys, when the page goes.
const KEYS_OFF: &str = r#"
if (window.__sbBooruKeys) removeEventListener("keydown", window.__sbBooruKeys);
window.__sbBooruKeys = null;
"#;
/// The page starts at its top. A picture the local viewer was carrying off
/// (motion.rs) has no tile to land on here.
const ARRIVE: &str = r#"
scrollTo({ top: 0, behavior: "instant" });
window.__sbHero?.drop();
"#;

/// Whether the site is to be asked about the post by itself: nothing is known
/// of it, it came without its file, or it is on a Gelbooru-kind site and came
/// without its tags' categories.
fn wants_full(post: Option<&RemotePost>, api: Option<BooruApi>) -> bool {
    post.is_none_or(|post| {
        post.file_url.trim().is_empty()
            || (api == Some(BooruApi::Gelbooru) && post.groups.is_empty())
    })
}

/// The post to show: what its site said when asked for it alone, else what
/// the search brought. The search's address for its page stays, as that is
/// what its import is found by, and so does what the answer left out.
fn merged(kept: Option<&RemotePost>, looked: Option<&RemotePost>) -> Option<RemotePost> {
    let (Some(kept), Some(looked)) = (kept, looked) else {
        return looked.or(kept).cloned();
    };
    let mut post = looked.clone();
    post.page_url = kept.page_url.clone();
    post.post_id = post.post_id.or(kept.post_id);
    if post.preview_url.is_empty() {
        post.preview_url = kept.preview_url.clone();
    }
    if post.groups.is_empty() {
        post.groups = kept.groups.clone();
    }
    Some(post)
}

/// The address of what the stage shows. A picture shows its sample, a copy
/// sized for a screen. A video plays its file, as its sample is often a
/// still, unless the file's site only answers its own pages: then the copy
/// behind the server is all a page of ours can show.
fn stage_source(post: &RemotePost) -> Option<&str> {
    fn filled(url: &str) -> Option<&str> {
        Some(url).filter(|url| !url.trim().is_empty())
    }
    let sample = post.sample_url.as_deref().and_then(filled);
    let file = filled(&post.file_url).filter(|url| guarded_by(url).is_none());
    if post.is_video() {
        file.or(sample)
    } else {
        sample.or(file).or_else(|| filled(&post.preview_url))
    }
}

/// The post's tags under their categories, in the viewer's order: artists,
/// copyrights, characters, the rest, meta. Each group by name.
fn grouped(post: &RemotePost) -> Vec<(TagCategory, Vec<String>)> {
    const ORDER: [TagCategory; 5] = [
        TagCategory::Artist,
        TagCategory::Copyright,
        TagCategory::Character,
        TagCategory::General,
        TagCategory::Meta,
    ];
    ORDER
        .into_iter()
        .filter_map(|category| {
            let mut tags: Vec<String> = post
                .tags
                .iter()
                .filter(|tag| post.groups.category(tag) == category)
                .cloned()
                .collect();
            tags.sort();
            (!tags.is_empty()).then_some((category, tags))
        })
        .collect()
}

/// Sizes the picture's box from the post's size, before the picture loads:
/// as wide as the page and the stage's height allow, and no larger than the
/// picture is. Nothing for a size the site didn't give.
fn fit_style(width: u32, height: u32) -> String {
    if width == 0 || height == 0 {
        return String::new();
    }
    format!(
        "aspect-ratio: {width} / {height}; width: min(100%, calc(var(--fit-h) * {width} / {height}), {width}px)"
    )
}

/// `url("…")` for inline CSS.
fn css_url(url: &str) -> String {
    format!(
        "url(\"{}\")",
        url.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// Whether a post's source is an address to link to.
fn is_link(source: &str) -> bool {
    ["http://", "https://"]
        .iter()
        .any(|scheme| source.len() > scheme.len() && source.starts_with(scheme))
}

/// One post on another site (`u` is its page), opened from the search `q` on
/// the sites `s`, which Back and the arrows go by.
#[component]
pub fn BooruPost(u: String, q: String, s: String) -> Element {
    // The post's page on its site, which is how it is known.
    let page = boorus::from_address(&u);
    let whose = (store::owner(), page.clone());
    let around = boorus::around(&page);
    let looked = LOOKED.read().get(&whose).cloned();
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // Where the keys go from, as the page stays while its post changes.
    let mut here = use_signal(|| (page.clone(), q.clone(), s.clone()));

    use_effect(use_reactive!(|u, q, s| {
        let page = boorus::from_address(&u);
        here.set((page.clone(), q, s));
        error.set(None);
        document::eval(ARRIVE);
        let whose = (store::owner(), page.clone());
        let around = boorus::around(&page);
        let known = around.as_ref().map(|a| &*a.found.post);
        if LOOKED.peek().contains_key(&whose)
            || !wants_full(known, around.as_ref().and_then(|a| a.api))
        {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            let answer = client.remote_post(&page).await;
            if answer.as_ref().is_err_and(|e| e.is_unauthorized()) {
                store::forget_session();
                return;
            }
            let mut looked = LOOKED.write();
            if looked.len() >= MAX_LOOKED {
                looked.clear();
            }
            looked.insert(whose, answer.map(Rc::new).map_err(|e| e.to_string()));
        });
    }));

    // Back to the results: where they were, or to them anew when this page
    // was the first one. The web build always says it can go back, so that
    // one finds out by trying.
    let step_back = move || {
        let (_, q, s) = here.peek().clone();
        let nav = navigator();
        if !nav.can_go_back() {
            nav.push(Route::Boorus { q, s });
            return;
        }
        let from = router().full_route_string();
        nav.go_back();
        spawn(async move {
            activity::wait_ms(250).await;
            if router().full_route_string() == from {
                navigator().push(Route::Boorus { q, s });
            }
        });
    };
    // Another post takes this one's place, so Back still leads to the results.
    let step = move |forward: bool| {
        let (page, q, s) = here.peek().clone();
        let next = boorus::around(&page).and_then(|a| if forward { a.next } else { a.prev });
        if let Some(next) = next {
            navigator().replace(Route::BooruPost {
                u: boorus::to_address(&next),
                q,
                s,
            });
        }
    };
    use_future(move || async move {
        let mut keys = document::eval(KEYS);
        while let Ok(key) = keys.recv::<String>().await {
            match key.as_str() {
                "ArrowLeft" => step(false),
                "ArrowRight" => step(true),
                "Escape" => step_back(),
                _ => {}
            }
        }
    });
    use_drop(|| {
        document::eval(KEYS_OFF);
    });

    // A post saved from here stays marked in the results it came from.
    use_effect(|| boorus::note_saved(&activity::jobs_by_source(&JOBS.read())));

    let kept = around.as_ref().map(|a| &*a.found.post);
    let answered = looked.as_ref().and_then(|answer| answer.as_ref().ok());
    let Some(post) = merged(kept, answered.map(|post| &**post)) else {
        let failed = looked.and_then(Result::err);
        return rsx! {
            div { class: "page booru-post",
                if let Some(message) = failed {
                    ErrorBanner { message }
                    Link { class: "btn text", to: Route::Boorus { q, s },
                        Ico { icon: Icon::ArrowBack }
                        {t!("post-back")}
                    }
                } else {
                    div { class: "sentinel", Spinner {} }
                }
            }
        };
    };
    let post = Rc::new(post);
    let status = boorus::status_of(&post, &activity::jobs_by_source(&JOBS.read()));
    let also: Vec<Twin> = around
        .as_ref()
        .map(|a| a.found.also.clone())
        .unwrap_or_default();
    let (prev, next) = around.map(|a| (a.prev, a.next)).unwrap_or_default();

    let client = store::client();
    let address = |url: &str| client.as_ref().map(|c| boorus::picture_src(c, url));
    let src = stage_source(&post).and_then(address);
    let preview = Some(post.preview_url.as_str())
        .filter(|url| !url.is_empty())
        .and_then(address);
    let alt = post.tags.join(" ");
    let groups = grouped(&post);
    let source = post.source.clone().filter(|source| !source.is_empty());
    let dimensions =
        (post.width > 0 && post.height > 0).then(|| format!("{} × {}", post.width, post.height));
    let size = post
        .file_size
        .map(|bytes| human_size(i64::try_from(bytes).unwrap_or(i64::MAX)));
    let saved = post.clone();
    let save = move |_| {
        if saving() {
            return;
        }
        let post = saved.clone();
        saving.set(true);
        error.set(None);
        spawn(async move {
            if let Err(message) = boorus::save_one(&post).await {
                error.set(Some(message));
            }
            saving.set(false);
        });
    };
    let failure = match &status {
        Some(Status::Failed(message)) => Some(message.clone()),
        _ => None,
    };

    rsx! {
        div { class: "page post-layout booru-post",
            div { class: "post-main",
                div { class: "stage",
                    if let Some(src) = src {
                        if post.is_video() {
                            video {
                                key: "{post.page_url}",
                                src,
                                poster: preview,
                                style: fit_style(post.width, post.height),
                                controls: true,
                                r#loop: true,
                                playsinline: true,
                                preload: "metadata",
                            }
                        } else {
                            StagePicture {
                                key: "{post.page_url}",
                                src,
                                preview,
                                alt,
                                width: post.width,
                                height: post.height,
                            }
                        }
                    } else {
                        Spinner {}
                    }
                    button { class: "stage-btn stage-back glass float", r#type: "button", title: t!("post-back"), aria_label: t!("post-back"),
                        onclick: move |_| step_back(),
                        Ico { icon: Icon::ArrowBack }
                    }
                    // Always there, greyed out at either end, so nothing comes and goes.
                    button { class: "stage-btn stage-prev glass float", r#type: "button", title: t!("post-prev-title"), aria_label: t!("post-prev"),
                        disabled: prev.is_none(),
                        onclick: move |_| step(false),
                        Ico { icon: Icon::ChevronLeft, size: 32 }
                    }
                    button { class: "stage-btn stage-next glass float", r#type: "button", title: t!("post-next-title"), aria_label: t!("post-next"),
                        disabled: next.is_none(),
                        onclick: move |_| step(true),
                        Ico { icon: Icon::ChevronRight, size: 32 }
                    }
                }
                div { class: "post-bar glass float", role: "toolbar", aria_label: t!("post-actions"),
                    match status {
                        Some(Status::Saved(Some(id), _)) => rsx! {
                            Link { class: "btn tonal", to: Route::PostView { id, q: String::new() },
                                Ico { icon: Icon::Check }
                                {t!("post-number", id = id.to_string())}
                            }
                        },
                        Some(Status::Saved(None, note)) => rsx! {
                            span { class: "pill ok", role: "status", title: "{note}", {t!("boorus-known")} }
                        },
                        Some(Status::Busy(percent)) => rsx! {
                            button { class: "btn", r#type: "button", disabled: true,
                                Ico { icon: Icon::Download }
                                if let Some(percent) = percent { "{percent}%" } else { {t!("upload-job-running")} }
                            }
                        },
                        Some(Status::Failed(_)) | None => rsx! {
                            button { class: "btn", r#type: "button", disabled: saving(), onclick: save,
                                Ico { icon: Icon::Download }
                                {t!("save")}
                            }
                        },
                    }
                    a { class: "btn text", href: "{post.page_url}", target: "_blank", rel: "noopener noreferrer",
                        Ico { icon: Icon::Link }
                        {t!("boorus-open-site")}
                    }
                }
                if let Some(message) = error().or(failure) {
                    ErrorBanner { message }
                }
            }
            aside { class: "side",
                if !groups.is_empty() {
                    div { class: "card",
                        h2 { class: "card-title", {t!("post-tags")} }
                        div { class: "tag-groups",
                            for (category, tags) in groups {
                                section { key: "{category_label(category)}", class: "tag-group",
                                    h3 { "{category_label(category)}" }
                                    div { class: "chips",
                                        for tag in tags {
                                            Link {
                                                key: "{tag}",
                                                class: category_class(category),
                                                to: boorus::search_route(&tag, &s),
                                                "{tag}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "card",
                    h2 { class: "card-title", {t!("post-details")} }
                    dl { class: "meta",
                        dt { {t!("boorus-site")} } dd { "{boorus::site_name(&post.site)}" }
                        if !also.is_empty() {
                            dt { {t!("boorus-also")} }
                            dd { class: "booru-also",
                                for twin in also {
                                    a { key: "{twin.page_url}", href: "{twin.page_url}", target: "_blank", rel: "noopener noreferrer",
                                        "{boorus::site_name(&twin.site)}"
                                    }
                                }
                            }
                        }
                        dt { {t!("rating")} } dd { {rating_label(post.rating)} }
                        if let Some(size) = size {
                            dt { {t!("post-size")} } dd { "{size}" }
                        }
                        if let Some(dimensions) = dimensions {
                            dt { {t!("post-dimensions")} } dd { "{dimensions}" }
                        }
                        if let Some(score) = post.score {
                            dt { {t!("boorus-score")} } dd { "{score}" }
                        }
                        if let Some(date) = post.created_at {
                            dt { {t!("post-added")} } dd { "{short_date(date)}" }
                        }
                        if let Some(source) = source {
                            dt { {t!("post-source")} }
                            dd { class: "break",
                                if is_link(&source) {
                                    a { href: "{source}", target: "_blank", rel: "noopener noreferrer", "{source}" }
                                } else {
                                    "{source}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The picture on the stage. Until it has loaded, the thumbnail the grid
/// already showed fills its box.
#[component]
fn StagePicture(
    src: String,
    preview: Option<String>,
    alt: String,
    width: u32,
    height: u32,
) -> Element {
    let mut loaded = use_signal(|| false);
    let waiting = !loaded() && preview.is_some();
    let mut style = fit_style(width, height);
    if let Some(preview) = preview.filter(|_| waiting) {
        style.push_str(&format!("; --thumb: {}", css_url(&preview)));
    }
    rsx! {
        img {
            class: if waiting { "loading" },
            src,
            alt,
            style,
            decoding: "async",
            "referrerpolicy": "no-referrer",
            onload: move |_| loaded.set(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbooru_core::Rating;

    fn post() -> RemotePost {
        RemotePost {
            id: 7,
            site: "https://d.org".into(),
            page_url: "https://d.org/posts/7".into(),
            preview_url: "https://cdn.d.org/360x360/a.jpg".into(),
            sample_url: Some("https://cdn.d.org/sample/a.jpg".into()),
            file_url: "https://cdn.d.org/original/a.png".into(),
            tags: vec![
                "sky".into(),
                "someone".into(),
                "miku".into(),
                "vocaloid".into(),
                "highres".into(),
                "cloud".into(),
            ],
            groups: Default::default(),
            rating: Rating::Safe,
            width: 1200,
            height: 800,
            md5: None,
            source: None,
            score: None,
            created_at: None,
            file_size: None,
            post_id: None,
        }
    }

    #[test]
    fn a_post_is_asked_for_alone_only_when_something_is_missing() {
        let mut p = post();
        p.groups.put(TagCategory::Artist, "someone");
        assert!(!wants_full(Some(&p), Some(BooruApi::Danbooru)));
        assert!(!wants_full(Some(&p), Some(BooruApi::Gelbooru)));
        assert!(!wants_full(Some(&p), None));
        // Opened from a link: nothing is known of it.
        assert!(wants_full(None, None));
        // Gelbooru's API names no categories; its page does.
        p.groups = Default::default();
        assert!(wants_full(Some(&p), Some(BooruApi::Gelbooru)));
        assert!(!wants_full(Some(&p), Some(BooruApi::Moebooru)));
        // And without a key it shows no file.
        p.file_url = " ".into();
        assert!(wants_full(Some(&p), Some(BooruApi::Danbooru)));
    }

    #[test]
    fn the_answer_fills_in_what_the_search_left_out() {
        let mut kept = post();
        kept.file_url = String::new();
        kept.post_id = Some(4);
        kept.page_url = "https://d.org/index.php?page=post&s=view&id=7".into();
        kept.groups.put(TagCategory::Artist, "someone");
        let mut looked = post();
        looked.preview_url = String::new();
        let both = merged(Some(&kept), Some(&looked)).unwrap();
        assert_eq!(both.file_url, "https://cdn.d.org/original/a.png");
        // Its import is found by the address the search gave.
        assert_eq!(both.page_url, kept.page_url);
        assert_eq!(both.post_id, Some(4));
        assert_eq!(both.preview_url, kept.preview_url);
        assert_eq!(both.groups.artist, ["someone"]);
        assert_eq!(merged(Some(&kept), None), Some(kept.clone()));
        assert_eq!(merged(None, Some(&looked)), Some(looked));
        assert_eq!(merged(None, None), None);
    }

    #[test]
    fn the_stage_shows_the_sample_or_plays_the_file() {
        let mut p = post();
        assert_eq!(stage_source(&p), Some("https://cdn.d.org/sample/a.jpg"));
        p.sample_url = None;
        assert_eq!(stage_source(&p), Some("https://cdn.d.org/original/a.png"));
        // No file yet: the thumbnail, until the site is asked.
        p.file_url = String::new();
        assert_eq!(stage_source(&p), Some("https://cdn.d.org/360x360/a.jpg"));
        p.preview_url = String::new();
        assert_eq!(stage_source(&p), None);

        // A video plays its file; its sample is often a still.
        let mut v = post();
        v.file_url = "https://cdn.d.org/original/a.webm".into();
        assert_eq!(stage_source(&v), Some("https://cdn.d.org/original/a.webm"));
        // Gelbooru's file servers refuse a page of ours: the copy behind the
        // server is the one to show.
        let relayed = "/api/v1/remote/media?u=x&exp=1&sig=ab";
        v.file_url = "https://video-cdn3.gelbooru.com/images/aa/bb/a.mp4".into();
        v.sample_url = Some(relayed.into());
        assert_eq!(stage_source(&v), Some(relayed));
        let mut g = post();
        g.file_url = "https://img4.gelbooru.com/images/aa/bb/a.png".into();
        g.sample_url = None;
        assert_eq!(stage_source(&g), Some("https://cdn.d.org/360x360/a.jpg"));
    }

    #[test]
    fn tags_go_under_their_categories() {
        let mut p = post();
        assert_eq!(grouped(&p).len(), 1, "no categories: one group");
        assert_eq!(grouped(&p)[0].0, TagCategory::General);
        p.groups.put(TagCategory::Artist, "someone");
        p.groups.put(TagCategory::Character, "miku");
        p.groups.put(TagCategory::Copyright, "vocaloid");
        p.groups.put(TagCategory::Meta, "highres");
        let groups = grouped(&p);
        let order: Vec<TagCategory> = groups.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            order,
            [
                TagCategory::Artist,
                TagCategory::Copyright,
                TagCategory::Character,
                TagCategory::General,
                TagCategory::Meta
            ]
        );
        assert_eq!(groups[3].1, ["cloud", "sky"], "by name");
        p.tags.clear();
        assert!(grouped(&p).is_empty());
    }

    #[test]
    fn the_box_is_sized_before_the_picture_loads() {
        assert_eq!(
            fit_style(1200, 800),
            "aspect-ratio: 1200 / 800; width: min(100%, calc(var(--fit-h) * 1200 / 800), 1200px)"
        );
        assert_eq!(fit_style(0, 800), "");
        assert_eq!(fit_style(1200, 0), "");
        assert_eq!(
            css_url("https://a.org/a\"b.jpg"),
            "url(\"https://a.org/a\\\"b.jpg\")"
        );
    }

    #[test]
    fn only_a_web_address_is_linked_as_the_source() {
        assert!(is_link("https://www.pixiv.net/artworks/1"));
        assert!(is_link("http://x.org"));
        for text in [
            "",
            "https://",
            "javascript:alert(1)",
            "pixiv 123",
            "ftp://x.org",
        ] {
            assert!(!is_link(text), "{text}");
        }
    }

    #[test]
    fn the_page_keeps_clear_of_the_viewers_machinery() {
        let source = include_str!("booru_post.rs");
        let (page, _) = source.split_once("\n#[cfg(test)]").unwrap();
        // The local viewer's picture is found by this id (motion.rs, post.rs).
        assert!(!page.contains("stage-img"));
        // Links to other sites open apart, and tell them nothing.
        assert_eq!(
            page.matches("target: \"_blank\"").count(),
            page.matches("rel: \"noopener noreferrer\"").count()
        );
        assert!(page.matches("target: \"_blank\"").count() >= 3);
        // Keys typed into the search box are the box's.
        assert!(KEYS.contains("INPUT|TEXTAREA|SELECT|VIDEO"));
        assert!(KEYS_OFF.contains("removeEventListener"));
    }
}
