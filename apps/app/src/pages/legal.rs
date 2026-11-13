//! Privacy Policy and Terms of Service. Both are open without signing in, so
//! the sign-in card can link to them. The text describes what the software
//! does; each server's operator is the one responsible for running it.

use crate::link::Link;
use dioxus::prelude::*;

use crate::Route;
use crate::brand::{Mark, Wordmark};

#[component]
pub fn Privacy() -> Element {
    rsx! {
        LegalPage { title: t!("privacy-title"),
            p { class: "lead",
                {t!("privacy-1")} }
            h2 { {t!("privacy-2")} }
            p {
                {t!("privacy-3")} }
            h2 { {t!("privacy-4")} }
            ul {
                li { {t!("privacy-5")} }
                li { {t!("privacy-6")} }
                li { {t!("privacy-7")} }
                li { {t!("privacy-8")} }
            }
            h2 { {t!("privacy-9")} }
            p {
                {t!("privacy-10")} }
            h2 { {t!("privacy-11")} }
            ul {
                li { {t!("privacy-12")} }
                li { {t!("privacy-13")} }
                li { {t!("privacy-14")} }
            }
            h2 { {t!("privacy-15")} }
            p {
                {t!("privacy-16")} }
            h2 { {t!("privacy-17")} }
            ul {
                li { {t!("privacy-18")} }
                li { {t!("privacy-19")} }
                li { {t!("privacy-20")} }
                li { {t!("privacy-21")} }
            }
        }
    }
}

#[component]
pub fn Terms() -> Element {
    rsx! {
        LegalPage { title: t!("terms-title"),
            p { class: "lead",
                {t!("terms-1")} }
            h2 { {t!("terms-2")} }
            ul {
                li { {t!("terms-3")} }
                li { {t!("terms-4")} }
            }
            h2 { {t!("terms-5")} }
            ul {
                li { {t!("terms-6")} }
                li { {t!("terms-7")} }
                li { {t!("terms-8")} }
            }
            h2 { {t!("terms-9")} }
            ul {
                li { {t!("terms-10")} }
                li { {t!("terms-11")} }
            }
            h2 { {t!("terms-12")} }
            p {
                {t!("terms-13")} }
        }
    }
}

/// Shared frame: a readable column, the brand and a way back.
#[component]
fn LegalPage(title: String, children: Element) -> Element {
    rsx! {
        div { class: "legal-page",
            article { class: "legal card",
                Link { class: "brand legal-brand", aria_label: "StarBooru", to: Route::Gallery { q: String::new() },
                    Mark { height: 30, compact: true }
                    Wordmark { height: 19 }
                }
                h1 { "{title}" }
                {children}
                nav { class: "legal-links", aria_label: t!("legal-nav"),
                    Link { to: Route::Privacy {}, {t!("privacy-title")} }
                    Link { to: Route::Terms {}, {t!("terms-title")} }
                    Link { to: Route::Gallery { q: String::new() }, {t!("legal-back")} }
                }
            }
        }
    }
}

/// Links under the sign-in card and in Settings.
#[component]
pub fn LegalLinks() -> Element {
    rsx! {
        nav { class: "legal-links", aria_label: t!("legal-nav"),
            Link { to: Route::Privacy {}, {t!("legal-privacy-short")} }
            Link { to: Route::Terms {}, {t!("legal-terms-short")} }
        }
    }
}
