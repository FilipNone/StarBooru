//! The search cheat sheet: every term of the query language with examples
//! that run a search, a box that explains a query as you type, and the
//! viewer's keyboard shortcuts.

use dioxus::prelude::*;
use starbooru_core::{FigureTerm, MediaKind, Order, Rating, SearchQuery, SortKey, parse_query};

use super::people::kind_label;
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::util::rating_label;

/// Example terms and what they do, grouped under a heading.
fn sections() -> [(String, Vec<(&'static str, String)>); 5] {
    [
        (
            t!("help-tags"),
            vec![
                ("blue_sky", t!("help-tag")),
                ("blue_sky night", t!("help-tag-all")),
                ("-cloud", t!("help-tag-not")),
                ("footwear", t!("help-tag-tree")),
                ("artist:some_name", t!("help-tag-category")),
                ("{girl blonde_hair boots}", t!("help-figure")),
                ("blue sky", t!("help-tag-spaces")),
                ("highres -sound", t!("help-tag-meta")),
            ],
        ),
        (
            t!("help-filters"),
            vec![
                ("rating:g", t!("help-rating")),
                ("rating:g rating:s", t!("help-rating-any")),
                ("-rating:e", t!("help-rating-not")),
                ("type:image", t!("help-type")),
                ("status:deleted", t!("help-deleted")),
            ],
        ),
        (
            t!("help-text"),
            vec![
                ("text:hello", t!("help-text-word")),
                ("text:\"no parking\"", t!("help-text-phrase")),
                ("-text:sale", t!("help-text-not")),
            ],
        ),
        (
            t!("help-sorting"),
            vec![
                ("order:new", t!("help-order-new")),
                ("order:score", t!("help-order-score")),
                ("order:size", t!("help-order-size")),
                ("order:pixels", t!("help-order-pixels")),
                ("order:tags", t!("help-order-tags")),
                ("order:updated", t!("help-order-updated")),
                ("order:size_asc", t!("help-order-direction")),
                ("order:random", t!("help-order-random")),
            ],
        ),
        (
            t!("help-combined"),
            vec![
                ("cat -dog rating:g order:score", t!("help-combined-cats")),
                ("type:video order:size", t!("help-combined-videos")),
            ],
        ),
    ]
}

fn shortcuts() -> [(String, String); 8] {
    [
        ("← →".into(), t!("key-prev-next")),
        ("E".into(), t!("key-edit")),
        ("F".into(), t!("key-fullscreen")),
        ("+ −".into(), t!("key-zoom")),
        ("0".into(), t!("key-fit")),
        ("Esc".into(), t!("key-escape")),
        (t!("key-double-click"), t!("key-double-click-action")),
        (t!("key-shift-click"), t!("key-shift-click-action")),
    ]
}

#[component]
pub fn Help() -> Element {
    let mut text = use_signal(String::new);
    let explained = explain(&text());
    let invalid = explained.is_err();
    rsx! {
        div { class: "page narrow",
            h1 { class: "page-title", {t!("help-title")} }

            section { class: "card",
                h2 { class: "card-title", {t!("help-try")} }
                form {
                    class: "try-query",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        navigator().push(Route::Gallery { q: text().trim().to_owned() });
                    },
                    input {
                        r#type: "search",
                        placeholder: "cat -dog rating:g order:score",
                        aria_label: t!("help-try-label"),
                        autocomplete: "off",
                        spellcheck: "false",
                        value: "{text}",
                        oninput: move |e| text.set(e.value()),
                    }
                    button { class: "btn", r#type: "submit", disabled: invalid,
                        Ico { icon: Icon::Search }
                        {t!("help-search")}
                    }
                }
                match explained {
                    Ok(lines) => rsx! {
                        ul { class: "explain", aria_live: "polite",
                            for line in lines {
                                li { key: "{line}", "{line}" }
                            }
                        }
                    },
                    Err(message) => rsx! {
                        p { class: "field-error", role: "alert", "{message}" }
                    },
                }
            }

            for (title, rows) in sections() {
                section { key: "{title}", class: "card",
                    h2 { class: "card-title", "{title}" }
                    dl { class: "syntax",
                        for (term, meaning) in rows {
                            dt { key: "{term}",
                                Link {
                                    class: "code-chip",
                                    to: Route::Gallery { q: term.to_owned() },
                                    title: t!("help-search-for", term = term),
                                    "{term}"
                                }
                            }
                            dd { "{meaning}" }
                        }
                    }
                }
            }

            section { class: "card",
                h2 { class: "card-title", {t!("help-tips")} }
                ul { class: "tips",
                    li { {t!("help-tip-normalise")} }
                    li { {t!("help-tip-aliases")} }
                    li { {t!("help-tip-terms")} }
                    li { {t!("help-tip-saved")} }
                }
            }

            section { class: "card",
                h2 { class: "card-title", {t!("help-shortcuts")} }
                dl { class: "syntax",
                    for (keys, action) in shortcuts() {
                        dt { key: "{keys}", kbd { "{keys}" } }
                        dd { "{action}" }
                    }
                }
            }
        }
    }
}

/// Plain-language reading of a query, one line per kind of term, or the
/// parser's error.
pub fn explain(text: &str) -> Result<Vec<String>, String> {
    let q: SearchQuery = parse_query(text).map_err(|e| e.to_string())?;
    let mut lines = Vec::new();
    if q.deleted {
        lines.push(t!("explain-deleted"));
    }
    match q.include.as_slice() {
        [] => {}
        [tag] => lines.push(t!("explain-tagged", tag = tag.as_str())),
        tags => lines.push(t!("explain-tagged-all", tags = tags.join(", "))),
    }
    match q.exclude.as_slice() {
        [] => {}
        [tag] => lines.push(t!("explain-not-tagged", tag = tag.as_str())),
        tags => lines.push(t!("explain-tagged-none", tags = tags.join(", "))),
    }
    if !q.ratings.is_empty() {
        lines.push(t!("explain-rated", ratings = ratings(&q.ratings)));
    }
    if !q.exclude_ratings.is_empty() {
        lines.push(t!(
            "explain-not-rated",
            ratings = ratings(&q.exclude_ratings)
        ));
    }
    for term in &q.figures {
        lines.push(figure_line(term, false));
    }
    for term in &q.exclude_figures {
        lines.push(figure_line(term, true));
    }
    for text in &q.text {
        lines.push(t!("explain-text", text = text.as_str()));
    }
    for text in &q.exclude_text {
        lines.push(t!("explain-not-text", text = text.as_str()));
    }
    if let Some(kind) = q.kind {
        lines.push(match kind {
            MediaKind::Image => t!("explain-images"),
            MediaKind::Animation => t!("explain-animations"),
            MediaKind::Video => t!("explain-videos"),
        });
    }
    if lines.is_empty() || (lines.len() == 1 && q.deleted) {
        lines.push(t!("explain-every"));
    }
    lines.push(order_phrase(q.order));
    Ok(lines)
}

/// One `{...}` term in words.
fn figure_line(term: &FigureTerm, negated: bool) -> String {
    let tags = term.tags.join(", ");
    let kind = term.kind.map(|k| kind_label(k).to_lowercase());
    match (kind, negated) {
        (None, false) => t!("explain-figure", tags = tags),
        (None, true) => t!("explain-no-figure", tags = tags),
        (Some(kind), false) if term.tags.is_empty() => t!("explain-figure-only", kind = kind),
        (Some(kind), true) if term.tags.is_empty() => t!("explain-no-figure-only", kind = kind),
        (Some(kind), false) => t!("explain-figure-kind", kind = kind, tags = tags),
        (Some(kind), true) => t!("explain-no-figure-kind", kind = kind, tags = tags),
    }
}

/// "general, sensitive or questionable".
fn ratings(list: &[Rating]) -> String {
    let names: Vec<String> = list.iter().map(|r| rating_label(*r)).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => {
            t!("list-or", rest = rest.join(", "), last = last.as_str())
        }
        _ => names.concat(),
    }
}

fn order_phrase(order: Order) -> String {
    let d = order.descending;
    match order.key {
        SortKey::Id if d => t!("explain-order-new"),
        SortKey::Id => t!("explain-order-old"),
        SortKey::Random => t!("explain-order-random"),
        SortKey::Score if d => t!("explain-order-score-desc"),
        SortKey::Score => t!("explain-order-score-asc"),
        SortKey::Size if d => t!("explain-order-size-desc"),
        SortKey::Size => t!("explain-order-size-asc"),
        SortKey::Pixels if d => t!("explain-order-pixels-desc"),
        SortKey::Pixels => t!("explain-order-pixels-asc"),
        SortKey::TagCount if d => t!("explain-order-tags-desc"),
        SortKey::TagCount => t!("explain-order-tags-asc"),
        SortKey::Updated if d => t!("explain-order-updated-desc"),
        SortKey::Updated => t!("explain-order-updated-asc"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_queries() {
        assert_eq!(explain("").unwrap(), ["Every post", "Newest first"]);
        assert_eq!(
            explain("cat -dog -bird rating:g rating:s -rating:e type:video order:size_asc")
                .unwrap(),
            [
                "Tagged cat",
                "Tagged with none of dog, bird",
                "Rated general or sensitive",
                "Not rated explicit",
                "Videos only",
                "By file size, smallest first",
            ]
        );
        assert_eq!(
            explain("status:deleted order:random").unwrap(),
            ["In the recycle bin", "Every post", "In random order"]
        );
        assert_eq!(
            explain("a b c order:old").unwrap(),
            ["Tagged with all of a, b, c", "Oldest first"]
        );
        assert_eq!(
            explain("{girl boots hat} {scarf} -{boy cap} -{sock} {other} -{girl}").unwrap(),
            [
                "A person (girl) has all of boots, hat",
                "One person has all of scarf",
                "A person marked as other",
                "No person (boy) has all of cap",
                "No one has all of sock",
                "No person marked as girl",
                "Newest first"
            ]
        );
        assert_eq!(
            explain(r#"text:"no parking" -text:sale"#).unwrap(),
            [
                "Text in the image has “no parking”",
                "Text in the image doesn’t have “sale”",
                "Newest first"
            ]
        );
        let ratings3 = explain("rating:g rating:s rating:q").unwrap();
        assert_eq!(ratings3[0], "Rated general, sensitive or questionable");
        assert!(explain("rating:z").unwrap_err().contains("unknown rating"));
    }

    #[test]
    fn every_example_parses() {
        for (_, rows) in sections() {
            for (term, _) in rows {
                assert!(parse_query(term).is_ok(), "{term}");
            }
        }
    }
}
