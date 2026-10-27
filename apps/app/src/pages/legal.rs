//! Privacy Policy and Terms of Service. Both are open without signing in, so
//! the sign-in card can link to them. The text describes what the software
//! does; each server's operator is the one responsible for running it.

use dioxus::prelude::*;

use crate::Route;
use crate::icons::{Ico, Icon};

#[component]
pub fn Privacy() -> Element {
    rsx! {
        LegalPage { title: "Privacy Policy",
            p { class: "lead",
                "StarBooru is software that runs on a server chosen by you or someone you trust. "
                "Nothing you store is sent to the StarBooru project or to anyone else."
            }
            h2 { "Who is responsible" }
            p {
                "The person or organisation that runs this server (the operator) decides who may sign up "
                "and is responsible for the data on it. Questions about your data go to them."
            }
            h2 { "What the server stores" }
            ul {
                li { "Your account: username, role and a salted Argon2id hash of your password. The password itself is never stored." }
                li { "Your sessions: a hash of each sign-in token, the kind of device it came from (for example “StarBooru app on Linux” or “Firefox on Android”), when it was created and last used, and when it expires." }
                li { "What you add: uploaded files and their size, dimensions and checksum, tags, ratings, sources, votes and saved searches." }
                li { "To slow down password guessing, the server counts failed sign-ins per account and per network address, and sign-ups per address. These counts are kept in memory for at most an hour and never written to disk." }
            }
            h2 { "What stays on your device" }
            p {
                "The app remembers the server address, your sign-in token and your preferences, such as the theme. "
                "Signing out removes the token."
            }
            h2 { "What StarBooru does not do" }
            ul {
                li { "No analytics, advertising or tracking, and no third-party services." }
                li { "The server only contacts other sites when you import from a URL. Those requests carry a generic user agent and none of your details." }
                li { "File contents, passwords and tokens are never written to the server's logs." }
            }
            h2 { "Who can see your posts" }
            p {
                "Everyone with an account on this server can see the posts on it. Media links handed to the app are signed "
                "and expire, so a copied link stops working within 12 hours."
            }
            h2 { "Keeping and deleting" }
            ul {
                li { "Deleted posts go to the recycle bin. They are erased when an administrator empties it, or automatically after the period the operator sets." }
                li { "Sessions end when you sign out, when they expire, or when you sign out other devices in Settings. Changing your password signs out every other device." }
                li { "To delete your account, ask an administrator of this server." }
                li { "The operator may keep backups, which follow their own schedule." }
            }
        }
    }
}

#[component]
pub fn Terms() -> Element {
    rsx! {
        LegalPage { title: "Terms of Service",
            p { class: "lead",
                "These terms cover your use of this StarBooru server. The operator who runs it may add to them."
            }
            h2 { "Your account" }
            ul {
                li { "Keep your password to yourself, and sign out devices you no longer use (Settings › Signed-in devices)." }
                li { "The first account on a server is its administrator. Administrators can manage tags, aliases and implications, and empty the recycle bin." }
            }
            h2 { "Your content" }
            ul {
                li { "Upload only what you have the right to keep and share with the other people on this server, and what is legal where the server runs." }
                li { "You keep every right to what you upload. You let the server store it, make thumbnails and previews, and show it to the accounts on this server." }
                li { "Originals are kept byte for byte. Rate posts honestly, so that rating filters work for everyone." }
            }
            h2 { "Fair use" }
            ul {
                li { "Don't try to reach other people's accounts or data, or to get around the server's limits." }
                li { "Don't overload the server with automated requests." }
            }
            h2 { "No warranty" }
            p {
                "StarBooru is provided as is, without warranty of any kind. Keep your own copy of anything you can't afford to lose. "
                "The operator can suspend accounts that break these terms and can change them; the current version is always on this page."
            }
        }
    }
}

/// Shared frame: a readable column, the brand and a way back.
#[component]
fn LegalPage(title: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "legal-page",
            article { class: "legal card",
                Link { class: "brand legal-brand", to: Route::Gallery { q: String::new() },
                    Ico { icon: Icon::Star }
                    span { "StarBooru" }
                }
                h1 { "{title}" }
                {children}
                nav { class: "legal-links", aria_label: "Legal",
                    Link { to: Route::Privacy {}, "Privacy Policy" }
                    Link { to: Route::Terms {}, "Terms of Service" }
                    Link { to: Route::Gallery { q: String::new() }, "Back to StarBooru" }
                }
            }
        }
    }
}

/// Links under the sign-in card and in Settings.
#[component]
pub fn LegalLinks() -> Element {
    rsx! {
        nav { class: "legal-links", aria_label: "Legal",
            Link { to: Route::Privacy {}, "Privacy" }
            Link { to: Route::Terms {}, "Terms" }
        }
    }
}
