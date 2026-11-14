//! A box for a list of tags that offers the collection's tags for the word
//! being typed, as the search box does. The offers are chips under the box:
//! a click or Tab puts the first one in place of the word, Esc puts them
//! away. A category prefix (`artist:mi`) stays and the rest is completed.

use super::shell::local_tags;
use crate::grip::Grip;
use dioxus::prelude::*;
use starbooru_core::{Tag, TagCategory, split_category};

/// The word the caret is in, and what of it a tag would replace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Word {
    /// Where the word starts and ends, in bytes.
    start: usize,
    end: usize,
    /// A category prefix that stays, such as `artist:`.
    lead: String,
    /// What is typed after it, to complete.
    typed: String,
}

/// The word around `caret`, a position in UTF-16 units as the page counts
/// them. `None` with the caret on a space, or in a word there is nothing to
/// complete in (`a:b` with an unknown `a`).
pub(crate) fn word_at(text: &str, caret: usize) -> Option<Word> {
    let mut units = 0usize;
    let mut at = text.len();
    for (i, c) in text.char_indices() {
        if units >= caret {
            at = i;
            break;
        }
        units = units.saturating_add(c.len_utf16());
    }
    let before = text.get(..at)?;
    let after = text.get(at..)?;
    let start = before.rfind(char::is_whitespace).map_or(0, |i| {
        i.saturating_add(
            before
                .get(i..)
                .and_then(|s| s.chars().next())
                .map_or(1, char::len_utf8),
        )
    });
    let end = at.saturating_add(after.find(char::is_whitespace).unwrap_or(after.len()));
    let word = text.get(start..end)?;
    if start == at {
        return None;
    }
    let (_, typed) = split_category(word);
    if typed.is_empty() || typed.contains(':') {
        return None;
    }
    Some(Word {
        start,
        end,
        lead: word.strip_suffix(typed).unwrap_or_default().to_owned(),
        typed: typed.to_owned(),
    })
}

/// `text` with `word` replaced by `name` and a space after it, and where the
/// caret goes, in UTF-16 units.
pub(crate) fn put_tag(text: &str, word: &Word, name: &str) -> (String, usize) {
    let before = text.get(..word.start).unwrap_or(text);
    let after = text.get(word.end..).unwrap_or_default();
    let mut next = format!("{before}{}{name}", word.lead);
    if !after.starts_with(char::is_whitespace) {
        next.push(' ');
    }
    let caret = next
        .encode_utf16()
        .count()
        .saturating_add(usize::from(after.starts_with(' ')));
    next.push_str(after);
    (next, caret)
}

/// The tags to offer: not the ones already in the box.
fn offers(text: &str, tags: Vec<Tag>) -> Vec<Tag> {
    let have: Vec<&str> = text
        .split_whitespace()
        .map(|w| split_category(w).1)
        .collect();
    tags.into_iter()
        .filter(|t| !have.contains(&t.name.as_str()))
        .collect()
}

fn category_class(category: TagCategory) -> &'static str {
    match category {
        TagCategory::General => "chip",
        TagCategory::Artist => "chip cat-artist",
        TagCategory::Character => "chip cat-character",
        TagCategory::Copyright => "chip cat-copyright",
        TagCategory::Meta => "chip cat-meta",
    }
}

/// Where the caret is in the box, in UTF-16 units.
async fn caret_of(id: &str) -> Option<usize> {
    let id = serde_json::to_string(id).ok()?;
    let at = document::eval(&format!(
        "return document.getElementById({id})?.selectionStart ?? -1;"
    ))
    .await
    .ok()?;
    at.as_u64().and_then(|n| usize::try_from(n).ok())
}

/// The tag box: a `textarea` with its grip, and the offers under it.
#[component]
pub fn TagBox(
    id: String,
    class: String,
    rows: u32,
    placeholder: String,
    value: String,
    oninput: EventHandler<String>,
) -> Element {
    let mut text = use_signal(|| value.clone());
    // Offers only follow typing: none for the tags the box opened with.
    let mut typing = use_signal(|| false);
    let mut caret_to = use_signal(|| None::<usize>);
    use_effect(use_reactive!(|value| {
        if *text.peek() != value {
            text.set(value);
        }
    }));
    let box_id = id.clone();
    let found = use_resource(move || {
        let id = box_id.clone();
        async move {
            let value = text();
            if !typing() {
                return None;
            }
            let word = word_at(&value, caret_of(&id).await?)?;
            let tags = local_tags(word.typed.clone()).await;
            Some((word, offers(&value, tags)))
        }
    });
    // Back in the box after a pick, the caret after the new tag. It runs
    // after the render that put the new text in.
    let caret_id = id.clone();
    use_effect(move || {
        let Some(at) = caret_to() else {
            return;
        };
        caret_to.set(None);
        let id = serde_json::to_string(&caret_id).unwrap_or_else(|_| "null".into());
        document::eval(&format!(
            "const box = document.getElementById({id}); if (box) {{ box.focus(); box.setSelectionRange({at}, {at}); }}"
        ));
    });
    let (word, tags) = found
        .read()
        .clone()
        .flatten()
        .filter(|(_, tags)| !tags.is_empty())
        .unzip();
    let tags: Vec<Tag> = tags.unwrap_or_default();
    let mut pick = move |word: Word, name: String| {
        let (next, at) = put_tag(&text.peek(), &word, &name);
        typing.set(false);
        text.set(next.clone());
        oninput.call(next);
        caret_to.set(Some(at));
    };
    let first = word.clone().zip(tags.first().map(|t| t.name.clone()));
    rsx! {
        span { class: "text-box",
            textarea {
                id: "{id}",
                class: "{class}",
                rows: "{rows}",
                placeholder: "{placeholder}",
                spellcheck: "false",
                autocapitalize: "none",
                autocomplete: "off",
                aria_autocomplete: "list",
                aria_controls: "{id}-offers",
                value: "{text}",
                oninput: move |e| {
                    let value = e.value();
                    typing.set(true);
                    text.set(value.clone());
                    oninput.call(value);
                },
                onblur: move |_| typing.set(false),
                onkeydown: move |e: KeyboardEvent| {
                    let Some((word, name)) = first.clone() else {
                        return;
                    };
                    match e.key() {
                        Key::Tab if !e.modifiers().contains(Modifiers::SHIFT) => {
                            e.prevent_default();
                            pick(word, name);
                        }
                        Key::Escape => {
                            // Only the offers go, not the popup round the box.
                            e.prevent_default();
                            e.stop_propagation();
                            typing.set(false);
                        }
                        _ => {}
                    }
                },
            }
            Grip {}
        }
        div { class: "tag-offers", id: "{id}-offers", role: "listbox", aria_label: t!("tag-offers"),
            if let Some(word) = word {
                for tag in tags {
                    button {
                        key: "{tag.name}",
                        r#type: "button",
                        role: "option",
                        class: category_class(tag.category),
                        tabindex: "-1",
                        // Keeps the caret in the box, which a press would take away.
                        onmousedown: move |e| e.prevent_default(),
                        onclick: {
                            let word = word.clone();
                            let name = tag.name.clone();
                            move |_| pick(word.clone(), name.clone())
                        },
                        "{tag.name}"
                        span { class: "count", "{tag.post_count}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, caret: usize) -> Option<(String, String)> {
        word_at(text, caret).map(|w| (w.lead, w.typed))
    }

    #[test]
    fn the_word_at_the_caret_is_completed() {
        assert_eq!(word("blue_sk", 7), Some(("".into(), "blue_sk".into())));
        assert_eq!(word("cat blu dog", 7), Some(("".into(), "blu".into())));
        // The caret inside a word takes the whole word.
        assert_eq!(word("cat blu dog", 5), Some(("".into(), "blu".into())));
        assert_eq!(word("artist:mi", 9), Some(("artist:".into(), "mi".into())));
        assert_eq!(
            word("cat\nchar:ri", 11),
            Some(("char:".into(), "ri".into()))
        );
        // UTF-16 positions: the caret counts 🐱 as two.
        assert_eq!(word("🐱 neko", 7), Some(("".into(), "neko".into())));
        for (text, caret) in [
            ("", 0),
            ("cat ", 4),
            ("cat dog", 0),
            ("cat  dog", 4),
            ("artist:", 7),
            ("rating:s", 8),
            ("a:b:c", 5),
        ] {
            assert_eq!(word(text, caret), None, "{text:?} at {caret}");
        }
        // A caret past the end is at the end.
        assert_eq!(word("cat", 99), Some(("".into(), "cat".into())));
    }

    #[test]
    fn a_picked_tag_takes_the_words_place_with_a_space_after() {
        let text = "cat blu dog";
        let w = word_at(text, 7).unwrap();
        assert_eq!(
            put_tag(text, &w, "blue_sky"),
            ("cat blue_sky dog".into(), 13)
        );
        let text = "cat blu";
        let w = word_at(text, 7).unwrap();
        assert_eq!(put_tag(text, &w, "blue_sky"), ("cat blue_sky ".into(), 13));
        let text = "artist:mi";
        let w = word_at(text, 9).unwrap();
        assert_eq!(put_tag(text, &w, "mika"), ("artist:mika ".into(), 12));
        let text = "🐱 ne\nx";
        let w = word_at(text, 5).unwrap();
        assert_eq!(put_tag(text, &w, "neko"), ("🐱 neko\nx".into(), 7));
    }

    #[test]
    fn tags_already_in_the_box_are_not_offered() {
        let tag = |name: &str| -> Tag {
            serde_json::from_value(serde_json::json!({
                "id": 1, "name": name, "category": "general", "post_count": 3,
            }))
            .unwrap()
        };
        let left = offers(
            "artist:mika blu",
            vec![tag("mika"), tag("blue_sky"), tag("blu")],
        );
        let names: Vec<&str> = left.iter().map(|t| t.name.as_str()).collect();
        // Nor is the word being typed when it is a whole tag already.
        assert_eq!(names, ["blue_sky"]);
    }
}
