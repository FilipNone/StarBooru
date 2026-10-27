use dioxus::prelude::*;
use starbooru_client::Client;
use starbooru_core::{Credentials, ServerInfo};

use crate::icons::{Ico, Icon};
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
                    error.set(Some(format!("Cannot reach the server: {e}")));
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
                    error.set(Some("Wrong username or password.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let first_user = info().is_some_and(|i| !i.has_users);
    let can_register = info().is_some_and(|i| i.registration_open && i.has_users);
    rsx! {
        div { class: "login-page",
            div { class: "aurora", aria_hidden: "true" }
            form { class: "login-card glass", onsubmit: submit,
                div { class: "login-brand",
                    span { class: "logo", Ico { icon: Icon::Star, size: 32 } }
                    h1 { "StarBooru" }
                    p { class: "muted",
                        if let Some(i) = info() { "{i.name} · v{i.version}" } else { "Your private image library" }
                    }
                }
                if first_user {
                    div { class: "banner info",
                        "This server has no accounts yet. The account you create now becomes the administrator."
                    }
                }
                label { class: "field",
                    span { "Server" }
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
                    span { "Username" }
                    input {
                        autocomplete: "username",
                        required: true,
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                    }
                }
                label { class: "field",
                    span { "Password" }
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
                        "Please wait…"
                    } else if first_user {
                        "Create administrator account"
                    } else if creating() {
                        "Create account"
                    } else {
                        "Sign in"
                    }
                }
                if can_register {
                    button {
                        class: "btn text wide",
                        r#type: "button",
                        onclick: move |_| creating.toggle(),
                        if creating() { "I already have an account" } else { "Create an account" }
                    }
                }
            }
        }
    }
}
