use dioxus::prelude::*;
use starbooru_client::Client;
use starbooru_core::{Credentials, ServerInfo};

use super::LegalLinks;
use crate::brand::{Mark, Wordmark};
use crate::icons::{Ico, Icon};
use crate::oidc::{self, Done};
use crate::store::{self, PREFS};

#[component]
pub fn Login() -> Element {
    let mut server = use_signal(|| {
        let last = PREFS.peek().last_server.clone();
        if last.is_empty() {
            store::default_server()
        } else {
            last
        }
    });
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut info = use_signal(|| None::<ServerInfo>);
    let mut creating = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // A provider sign-in waiting for the browser (desktop and Android).
    let mut waiting = use_signal(|| None::<dioxus::core::Task>);

    let probe = move || {
        let url = server.peek().trim().to_owned();
        if url.is_empty() {
            return;
        }
        spawn(async move {
            let result = match Client::new(&url) {
                Ok(client) => client.info().await.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            match result {
                Ok(i) => {
                    creating.set(!i.has_users);
                    info.set(Some(i));
                    error.set(None);
                }
                Err(e) => {
                    info.set(None);
                    error.set(Some(t!("login-unreachable", error = e.to_string())));
                }
            }
        });
    };
    use_hook(probe);

    let submit = move |e: FormEvent| {
        e.prevent_default();
        if busy() {
            return;
        }
        let url = server().trim().to_owned();
        let creds = Credentials {
            username: username().trim().to_owned(),
            password: password(),
        };
        let register = creating();
        busy.set(true);
        error.set(None);
        spawn(async move {
            let result = match Client::new(&url) {
                Ok(client) if register => client.register(&creds).await,
                Ok(client) => client.login(&creds).await,
                Err(e) => Err(e),
            };
            busy.set(false);
            match result {
                Ok(auth) => store::sign_in(url, auth),
                Err(e) if e.is_unauthorized() => {
                    error.set(Some(t!("login-wrong")));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let sso = move |_| {
        let url = server().trim().to_owned();
        let client = match Client::new(&url) {
            Ok(client) => client,
            Err(e) => {
                error.set(Some(e.to_string()));
                return;
            }
        };
        busy.set(true);
        error.set(None);
        let task = spawn(async move {
            match oidc::run(client, url.clone(), false).await {
                // The page is on its way to the provider.
                Ok(None) => return,
                Ok(Some(Done::SignedIn(auth))) => store::sign_in(url, auth),
                Ok(Some(Done::Linked(_))) => {}
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
            waiting.set(None);
        });
        // The web page leaves instead of waiting.
        if cfg!(not(target_arch = "wasm32")) {
            waiting.set(Some(task));
        }
    };
    let cancel = move |_| {
        if let Some(task) = waiting.take() {
            task.cancel();
        }
        busy.set(false);
    };

    let first_user = info().is_some_and(|i| !i.has_users);
    let can_register = info().is_some_and(|i| i.registration_open && i.has_users);
    rsx! {
        div { class: "login-page",
            div { class: "aurora", aria_hidden: "true" }
            form { class: "login-card glass", onsubmit: submit,
                div { class: "login-brand",
                    span { class: "logo", Mark { height: 40 } }
                    h1 { aria_label: "StarBooru", Wordmark { height: 27 } }
                    p { class: "muted",
                        if let Some(i) = info() { "{i.name} · v{i.version}" } else { {t!("login-tagline")} }
                    }
                }
                if first_user {
                    div { class: "banner info",
                        {t!("login-first-user")}
                    }
                }
                label { class: "field",
                    span { {t!("login-server")} }
                    input {
                        r#type: "url",
                        inputmode: "url",
                        placeholder: "https://booru.example.com",
                        required: true,
                        value: "{server}",
                        oninput: move |e| server.set(e.value()),
                        onchange: move |_| probe(),
                    }
                }
                label { class: "field",
                    span { {t!("login-username")} }
                    input {
                        autocomplete: "username",
                        required: true,
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                    }
                }
                label { class: "field",
                    span { {t!("login-password")} }
                    input {
                        r#type: "password",
                        autocomplete: if creating() { "new-password" } else { "current-password" },
                        required: true,
                        minlength: if creating() { "8" } else { "1" },
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                    }
                }
                if let Some(e) = error() {
                    div { class: "banner error", role: "alert", "{e}" }
                }
                button { class: "btn wide", r#type: "submit", disabled: busy(),
                    if busy() {
                        {t!("login-wait")}
                    } else if first_user {
                        {t!("login-create-admin")}
                    } else if creating() {
                        {t!("login-create")}
                    } else {
                        {t!("login-sign-in")}
                    }
                }
                if let Some(name) = info().and_then(|i| i.oidc) {
                    div { class: "or", span { {t!("login-or")} } }
                    if waiting().is_some() {
                        p { class: "muted status-line", role: "status", {t!("oidc-waiting")} }
                        button { class: "btn tonal wide", r#type: "button", onclick: cancel,
                            {t!("cancel")}
                        }
                    } else {
                        button {
                            class: "btn tonal wide",
                            r#type: "button",
                            disabled: busy(),
                            onclick: sso,
                            Ico { icon: Icon::Shield }
                            {t!("login-with", provider = name)}
                        }
                    }
                }
                if can_register {
                    button {
                        class: "btn text wide",
                        r#type: "button",
                        onclick: move |_| creating.toggle(),
                        if creating() { {t!("login-have-account")} } else { {t!("login-want-account")} }
                    }
                }
                LegalLinks {}
            }
        }
    }
}
