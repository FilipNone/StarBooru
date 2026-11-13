//! The app's pages and their addresses.
//!
//! In a module of its own for the allow below: Rust 1.100 finds an unreachable
//! call in the code `#[derive(Routable)]` writes (dioxus-router-macro 0.7.10),
//! and a lint attribute on the enum doesn't reach the impls the derive adds.
#![allow(unreachable_code)]

use dioxus::prelude::*;

use crate::oidc::OidcReturn;
use crate::pages::{
    Admin, BooruPost, Boorus, Duplicates, Explore, Gallery, Help, NotFound, PostView, Privacy,
    Settings, SharedPage, Shell, TagPage, Tags, Terms, Trash, Upload,
};

#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/?:q")]
        Gallery { q: String },
        #[route("/explore?:q")]
        Explore { q: String },
        #[route("/post/:id?:q")]
        PostView { id: i64, q: String },
        #[route("/tags")]
        Tags {},
        #[route("/tag?:name")]
        TagPage { name: String },
        #[route("/upload")]
        Upload {},
        // `s` is the sites searched and `u` a post's page on its site. They and
        // `q` are written by `boorus::to_address`, as the router leaves `&` and
        // `%` in a value as they are.
        #[route("/boorus?:q&:s")]
        Boorus { q: String, s: String },
        #[route("/boorus/post?:u&:q&:s")]
        BooruPost { u: String, q: String, s: String },
        #[route("/settings")]
        Settings {},
        #[route("/trash")]
        Trash {},
        #[route("/duplicates")]
        Duplicates {},
        #[route("/help")]
        Help {},
        #[route("/admin")]
        Admin {},
    #[end_layout]
    #[route("/oidc?:oidc_code&:oidc_error")]
    OidcReturn { oidc_code: String, oidc_error: String },
    #[route("/s/:token")]
    SharedPage { token: String },
    #[route("/privacy")]
    Privacy {},
    #[route("/terms")]
    Terms {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}
