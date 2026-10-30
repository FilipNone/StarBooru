//! The viewer's "People" card: who is in the picture, each with their
//! clothing and looks by slot, and an editor for them. Saving goes through
//! `PUT /posts/{id}/figures`; the server keeps the post's tags in step.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use starbooru_core::{Figure, FigureKind, Post, Slot};

use super::ErrorBanner;
use super::post::use_ai_status;
use super::select::{Choice, Select};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;

/// A figure as the editor holds it: tag lists as typed.
#[derive(Clone, Debug, Default, PartialEq)]
struct Draft {
    kind: FigureKind,
    character: String,
    cosplay: String,
    slots: BTreeMap<Slot, String>,
}

impl Draft {
    fn of(figure: &Figure) -> Self {
        Self {
            kind: figure.kind,
            character: figure.character.clone().unwrap_or_default(),
            cosplay: figure.cosplay.clone().unwrap_or_default(),
            slots: figure
                .tags
                .iter()
                .map(|(slot, tags)| (*slot, tags.join(" ")))
                .collect(),
        }
    }

    fn figure(&self) -> Figure {
        let text = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
        Figure {
            kind: self.kind,
            character: text(&self.character),
            cosplay: text(&self.cosplay),
            tags: self
                .slots
                .iter()
                .map(|(slot, text)| {
                    let tags: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
                    (*slot, tags)
                })
                .filter(|(_, tags)| !tags.is_empty())
                .collect(),
        }
    }
}

pub(super) fn kind_label(kind: FigureKind) -> String {
    match kind {
        FigureKind::Girl => t!("people-girl"),
        FigureKind::Boy => t!("people-boy"),
        FigureKind::Other => t!("people-other"),
    }
}

fn slot_label(slot: Slot) -> String {
    match slot {
        Slot::Head => t!("slot-head"),
        Slot::Body => t!("slot-body"),
        Slot::Bottom => t!("slot-bottom"),
        Slot::Shoes => t!("slot-shoes"),
        Slot::Wear => t!("slot-wear"),
        Slot::Shape => t!("slot-shape"),
        Slot::Eyes => t!("slot-eyes"),
        Slot::Skin => t!("slot-skin"),
        Slot::Hair => t!("slot-hair"),
        Slot::Pose => t!("slot-pose"),
        Slot::Other => t!("slot-other"),
    }
}

/// Numbers figures within their kind when there are several of it:
/// `[1, 1, 2]` for girl, boy, girl; `0` for the only one of a kind.
fn numbers(kinds: &[FigureKind]) -> Vec<usize> {
    let mut seen: Vec<FigureKind> = Vec::new();
    kinds
        .iter()
        .map(|kind| {
            seen.push(*kind);
            if kinds.iter().filter(|k| *k == kind).count() > 1 {
                seen.iter().filter(|k| *k == kind).count()
            } else {
                0
            }
        })
        .collect()
}

fn label(kind: FigureKind, number: usize) -> String {
    if number == 0 {
        kind_label(kind)
    } else {
        t!("people-numbered", kind = kind_label(kind), n = number)
    }
}

#[component]
pub fn People(
    id: i64,
    figures: Vec<Figure>,
    deleted: bool,
    onsaved: EventHandler<Post>,
) -> Element {
    let ai = use_ai_status().tagging;
    let mut editing = use_signal(|| false);
    let mut drafts = use_signal(Vec::<Draft>::new);
    let mut busy = use_signal(|| false);
    let mut asking = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut none_found = use_signal(|| false);

    let start = {
        let figures = figures.clone();
        move |_| {
            let mut list: Vec<Draft> = figures.iter().map(Draft::of).collect();
            if list.is_empty() {
                list.push(Draft::default());
            }
            drafts.set(list);
            error.set(None);
            none_found.set(false);
            editing.set(true);
        }
    };
    let save = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let figures: Vec<Figure> = drafts.peek().iter().map(Draft::figure).collect();
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.set_figures(id, figures).await {
                Ok(post) => {
                    editing.set(false);
                    onsaved.call(post);
                }
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let suggest = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        asking.set(true);
        error.set(None);
        none_found.set(false);
        spawn(async move {
            match client.suggest_tags(id).await {
                Ok(s) if s.figures.is_empty() => none_found.set(true),
                Ok(s) => drafts.set(s.figures.iter().map(Draft::of).collect()),
                Err(e) => error.set(Some(e.to_string())),
            }
            asking.set(false);
        });
    };

    if editing() {
        let list = drafts();
        let kinds: Vec<FigureKind> = list.iter().map(|d| d.kind).collect();
        let numbers = numbers(&kinds);
        let choices: Vec<Choice> = FigureKind::ALL
            .into_iter()
            .map(|k| Choice::new(k.as_str(), kind_label(k)))
            .collect();
        return rsx! {
            form { class: "card edit people-edit", onsubmit: save,
                h2 { class: "card-title", {t!("people-title")} }
                p { class: "muted hint", {t!("people-hint")} }
                for (i, draft) in list.into_iter().enumerate() {
                    fieldset { key: "{i}", class: "figure-edit",
                        legend { {label(draft.kind, numbers.get(i).copied().unwrap_or(0))} }
                        div { class: "figure-row",
                            div { class: "field",
                                span { {t!("people-kind")} }
                                Select {
                                    class: "boxed",
                                    value: draft.kind.as_str(),
                                    options: choices.clone(),
                                    aria_label: t!("people-kind"),
                                    onchange: move |v: String| {
                                        if let (Some(d), Some(kind)) = (drafts.write().get_mut(i), FigureKind::parse(&v)) {
                                            d.kind = kind;
                                        }
                                    },
                                }
                            }
                            label { class: "field",
                                span { {t!("people-character")} }
                                input {
                                    value: "{draft.character}",
                                    spellcheck: "false",
                                    oninput: move |e| {
                                        if let Some(d) = drafts.write().get_mut(i) {
                                            d.character = e.value();
                                        }
                                    },
                                }
                            }
                            label { class: "field",
                                span { {t!("people-cosplay")} }
                                input {
                                    value: "{draft.cosplay}",
                                    spellcheck: "false",
                                    oninput: move |e| {
                                        if let Some(d) = drafts.write().get_mut(i) {
                                            d.cosplay = e.value();
                                        }
                                    },
                                }
                            }
                            button {
                                class: "icon-btn danger",
                                r#type: "button",
                                title: t!("people-remove"),
                                aria_label: t!("people-remove"),
                                onclick: move |_| {
                                    let mut list = drafts.write();
                                    if i < list.len() {
                                        list.remove(i);
                                    }
                                },
                                Ico { icon: Icon::Delete }
                            }
                        }
                        for (group, slots) in [
                            (t!("people-clothing"), &Slot::ALL[..5]),
                            (t!("people-looks"), &Slot::ALL[5..]),
                        ] {
                            h3 { class: "slot-group", "{group}" }
                            div { class: "slot-grid",
                                for slot in slots.iter().copied() {
                                    label { key: "{slot.as_str()}", class: "field",
                                        span { {slot_label(slot)} }
                                        input {
                                            value: draft.slots.get(&slot).cloned().unwrap_or_default(),
                                            spellcheck: "false",
                                            oninput: move |e| {
                                                if let Some(d) = drafts.write().get_mut(i) {
                                                    d.slots.insert(slot, e.value());
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "people-tools",
                    button {
                        class: "btn tonal",
                        r#type: "button",
                        disabled: drafts.read().len() >= starbooru_core::MAX_FIGURES,
                        onclick: move |_| drafts.write().push(Draft::default()),
                        Ico { icon: Icon::Add }
                        {t!("people-add")}
                    }
                    if ai {
                        button { class: "btn text", r#type: "button", disabled: asking(), onclick: suggest,
                            if asking() { {t!("ai-suggesting")} } else { {t!("ai-suggest-people")} }
                        }
                    }
                }
                if none_found() {
                    p { class: "muted hint", {t!("ai-no-people")} }
                }
                if let Some(message) = error() {
                    ErrorBanner { message }
                }
                div { class: "row-end",
                    button { class: "btn text", r#type: "button", onclick: move |_| editing.set(false), {t!("cancel")} }
                    button { class: "btn", r#type: "submit", disabled: busy(), Ico { icon: Icon::Check } {t!("save")} }
                }
            }
        };
    }

    let kinds: Vec<FigureKind> = figures.iter().map(|f| f.kind).collect();
    let numbers = numbers(&kinds);
    rsx! {
        div { class: "card people",
            div { class: "card-head",
                h2 { class: "card-title", {t!("people-title")} }
                if !deleted {
                    button { class: "btn text", onclick: start,
                        if figures.is_empty() {
                            Ico { icon: Icon::Add }
                            {t!("people-add")}
                        } else {
                            Ico { icon: Icon::Edit }
                            {t!("people-edit")}
                        }
                    }
                }
            }
            if figures.is_empty() {
                p { class: "muted hint", {t!("people-none")} }
            }
            div { class: "figures",
                for (i, figure) in figures.into_iter().enumerate() {
                    section { key: "{i}", class: "figure",
                        div { class: "figure-head",
                            span { class: "figure-kind", {label(figure.kind, numbers.get(i).copied().unwrap_or(0))} }
                            if let Some(name) = figure.character.clone() {
                                Link { class: "chip cat-character", to: Route::Gallery { q: name.clone() }, "{name}" }
                            }
                            if let Some(name) = figure.cosplay.clone() {
                                span { class: "muted", {t!("people-as")} }
                                Link {
                                    class: "chip cat-character",
                                    to: Route::Gallery { q: starbooru_core::cosplay_tag(&name) },
                                    "{name}"
                                }
                            }
                        }
                        if !figure.tags.is_empty() {
                            dl { class: "figure-slots",
                                for (slot, tags) in figure.tags {
                                    dt { key: "{slot.as_str()}", {slot_label(slot)} }
                                    dd { class: "chips",
                                        for tag in tags {
                                            Link { key: "{tag}", class: "chip", to: Route::Gallery { q: tag.clone() }, "{tag}" }
                                        }
                                    }
                                }
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
    use super::*;

    #[test]
    fn drafts_round_trip() {
        let figure = Figure {
            kind: FigureKind::Boy,
            character: Some("link".into()),
            cosplay: None,
            tags: BTreeMap::from([(Slot::Hair, vec!["blonde_hair".into(), "ponytail".into()])]),
        };
        let draft = Draft::of(&figure);
        assert_eq!(draft.slots[&Slot::Hair], "blonde_hair ponytail");
        assert_eq!(draft.figure(), figure);
        let mut typed = draft;
        typed.cosplay = "  ".into();
        typed.slots.insert(Slot::Shoes, "  boots   sandals ".into());
        typed.slots.insert(Slot::Pose, " ".into());
        let figure = typed.figure();
        assert_eq!(figure.cosplay, None);
        assert_eq!(figure.tags[&Slot::Shoes], ["boots", "sandals"]);
        assert!(!figure.tags.contains_key(&Slot::Pose));
    }

    #[test]
    fn numbers_count_within_each_kind() {
        use FigureKind::{Boy, Girl, Other};
        assert_eq!(numbers(&[Girl, Boy, Girl]), [1, 0, 2]);
        assert_eq!(numbers(&[Other]), [0]);
        assert!(numbers(&[]).is_empty());
    }
}
