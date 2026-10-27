//! The search cheat sheet: every term of the query language with examples
//! that run a search, a box that explains a query as you type, and the
//! viewer's keyboard shortcuts.

use dioxus::prelude::*;
use starbooru_core::{MediaKind, Order, Rating, SearchQuery, SortKey, parse_query};

use crate::Route;
use crate::icons::{Ico, Icon};

/// Example term and what it does, grouped under a heading.
const SECTIONS: [(&str, &[(&str, &str)]); 4] = [
    (
        "Tags",
        &[
            ("blue_sky", "has the tag"),
            ("blue_sky night", "has every tag listed"),
            ("-cloud", "doesn't have the tag"),
            (
                "blue sky",
                "two tags: spaces separate terms, so write blue_sky",
            ),
        ],
    ),
    (
        "Filters",
        &[
            (
                "rating:g",
                "rated general (g), sensitive (s), questionable (q) or explicit (e)",
            ),
            ("rating:g rating:s", "any of the listed ratings"),
            ("-rating:e", "any rating but explicit"),
            (
                "type:image",
                "still images; also type:animation and type:video",
            ),
            ("status:deleted", "posts in the recycle bin"),
        ],
    ),
    (
        "Sorting",
        &[
            (
                "order:new",
                "newest first (the default); order:old for oldest",
            ),
            ("order:score", "highest score first"),
            ("order:size", "largest file first"),
            ("order:pixels", "highest resolution first"),
            ("order:tags", "most tags first"),
            ("order:updated", "most recently edited first"),
            ("order:size_asc", "add _asc or _desc to set the direction"),
            ("order:random", "a new shuffle every time"),
        ],
    ),
    (
        "Combined",
        &[
            (
                "cat -dog rating:g order:score",
                "safe cats without dogs, best first",
            ),
            ("type:video order:size", "the biggest videos"),
        ],
    ),
];

const SHORTCUTS: [(&str, &str); 8] = [
    ("← →", "previous or next post"),
    ("E", "edit tags and rating"),
    ("F", "full screen"),
    ("+ −", "zoom in or out"),
    ("0", "fit to the screen"),
    (
        "Esc",
        "reset zoom, leave full screen, then back to the gallery",
    ),
    ("Double click", "zoom in on a point, again to fit"),
    ("Shift + click", "select a range in selection mode"),
];

#[component]
pub fn Help() -> Element {
    let mut text = use_signal(String::new);
    let explained = explain(&text());
    let invalid = explained.is_err();
    rsx! {
        div { class: "page narrow",
            h1 { class: "page-title", "Search cheat sheet" }

            section { class: "card",
                h2 { class: "card-title", "Try a query" }
                form {
                    class: "try-query",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        navigator().push(Route::Gallery { q: text().trim().to_owned() });
                    },
                    input {
                        r#type: "search",
                        placeholder: "cat -dog rating:g order:score",
                        aria_label: "Query to explain",
                        autocomplete: "off",
                        spellcheck: "false",
                        value: "{text}",
                        oninput: move |e| text.set(e.value()),
                    }
                    button { class: "btn", r#type: "submit", disabled: invalid,
                        Ico { icon: Icon::Search }
                        "Search"
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

            for (title, rows) in SECTIONS {
                section { key: "{title}", class: "card",
                    h2 { class: "card-title", "{title}" }
                    dl { class: "syntax",
                        for (term, meaning) in rows.iter().copied() {
                            dt { key: "{term}",
                                Link {
                                    class: "code-chip",
                                    to: Route::Gallery { q: term.to_owned() },
                                    title: "Search for {term}",
                                    "{term}"
                                }
                            }
                            dd { "{meaning}" }
                        }
                    }
                }
            }

            section { class: "card",
                h2 { class: "card-title", "Good to know" }
                ul { class: "tips",
                    li { "Tags are lower-cased, and spaces or repeated underscores become one _." }
                    li { "An alias finds its tag, and implied tags are added when you tag a post." }
                    li { "A query holds at most 40 terms." }
                    li { "Star a search in the gallery to keep it as a saved search." }
                }
            }

            section { class: "card",
                h2 { class: "card-title", "Keyboard shortcuts" }
                dl { class: "syntax",
                    for (keys, action) in SHORTCUTS {
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
        lines.push("In the recycle bin".to_owned());
    }
    match q.include.as_slice() {
        [] => {}
        [tag] => lines.push(format!("Tagged {tag}")),
        tags => lines.push(format!("Tagged with all of {}", tags.join(", "))),
    }
    match q.exclude.as_slice() {
        [] => {}
        [tag] => lines.push(format!("Not tagged {tag}")),
        tags => lines.push(format!("Tagged with none of {}", tags.join(", "))),
    }
    if !q.ratings.is_empty() {
        lines.push(format!("Rated {}", ratings(&q.ratings, "or")));
    }
    if !q.exclude_ratings.is_empty() {
        lines.push(format!("Not rated {}", ratings(&q.exclude_ratings, "or")));
    }
    if let Some(kind) = q.kind {
        lines.push(
            match kind {
                MediaKind::Image => "Still images only",
                MediaKind::Animation => "Animations only",
                MediaKind::Video => "Videos only",
            }
            .to_owned(),
        );
    }
    if lines.is_empty() || (lines.len() == 1 && q.deleted) {
        lines.push("Every post".to_owned());
    }
    lines.push(order_phrase(q.order));
    Ok(lines)
}

fn ratings(list: &[Rating], joiner: &str) -> String {
    let names: Vec<&str> = list.iter().map(|r| r.as_str()).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => {
            format!("{} {joiner} {last}", rest.join(", "))
        }
        _ => names.join(""),
    }
}

fn order_phrase(order: Order) -> String {
    let (key, high, low) = match order.key {
        SortKey::Id => {
            return if order.descending {
                "Newest first"
            } else {
                "Oldest first"
            }
            .into();
        }
        SortKey::Random => return "In random order".into(),
        SortKey::Score => ("score", "highest", "lowest"),
        SortKey::Size => ("file size", "largest", "smallest"),
        SortKey::Pixels => ("resolution", "highest", "lowest"),
        SortKey::TagCount => ("tag count", "most", "fewest"),
        SortKey::Updated => ("last edit", "latest", "earliest"),
    };
    let first = if order.descending { high } else { low };
    format!("By {key}, {first} first")
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
        let ratings3 = explain("rating:g rating:s rating:q").unwrap();
        assert_eq!(ratings3[0], "Rated general, sensitive or questionable");
        assert!(explain("rating:z").unwrap_err().contains("unknown rating"));
    }

    #[test]
    fn every_example_parses() {
        for (_, rows) in SECTIONS {
            for (term, _) in rows {
                assert!(parse_query(term).is_ok(), "{term}");
            }
        }
    }
}
