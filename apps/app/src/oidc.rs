//! Signing in with the server's identity provider (OpenID Connect).
//!
//! Web: the page leaves for the provider and comes back to `/oidc`, where
//! [`OidcReturn`] finishes; what it needs waits in `sessionStorage`.
//! Desktop/Android: the system browser opens the provider, and the answer
//! comes back to a one-off listener on `127.0.0.1` that this app runs.

use dioxus::prelude::*;
use starbooru_client::{Client, ClientError};
use starbooru_core::{AuthResponse, Identity, OidcError, OidcFinish};

/// How a provider sign-in ended.
pub enum Done {
    SignedIn(AuthResponse),
    Linked(Identity),
}

fn return_error(e: OidcError) -> String {
    match e {
        OidcError::Denied => t!("oidc-denied"),
        OidcError::Failed => t!("oidc-failed"),
    }
}

fn finish_error(e: &ClientError) -> String {
    match e {
        ClientError::Api { status: 400, .. } => t!("oidc-expired"),
        e => e.to_string(),
    }
}

/// Trades the code for a session, or links it to the signed-in account.
async fn finish(client: &Client, link: bool, code: String, handle: String) -> Result<Done, String> {
    let req = OidcFinish { code, handle };
    if link {
        client.oidc_link(&req).await.map(Done::Linked)
    } else {
        client.oidc_finish(&req).await.map(Done::SignedIn)
    }
    .map_err(|e| finish_error(&e))
}

/// Starts a provider sign-in on `client` (signed in when `link`). On the
/// web the page then leaves for the provider and this answers `Ok(None)`;
/// elsewhere it waits for the browser and answers how it ended.
pub async fn run(client: Client, server: String, link: bool) -> Result<Option<Done>, String> {
    platform::run(client, server, link).await
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use serde::{Deserialize, Serialize};
    use starbooru_client::Client;

    use super::Done;

    const KEY: &str = "starbooru-oidc";

    /// What the page needs once it is back from the provider.
    #[derive(Serialize, Deserialize)]
    pub struct Pending {
        pub server: String,
        pub handle: String,
        pub link: bool,
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.session_storage().ok()?
    }

    /// The pending sign-in, removed so it is only used once.
    pub fn take() -> Option<Pending> {
        let storage = storage()?;
        let json = storage.get_item(KEY).ok()??;
        let _ = storage.remove_item(KEY);
        serde_json::from_str(&json).ok()
    }

    pub async fn run(client: Client, server: String, link: bool) -> Result<Option<Done>, String> {
        let location = web_sys::window()
            .map(|w| w.location())
            .ok_or_else(|| t!("oidc-failed"))?;
        let origin = location.origin().map_err(|_| t!("oidc-failed"))?;
        let return_to = format!("{origin}/oidc");
        let start = if link {
            client.oidc_begin_link(&return_to).await
        } else {
            client.oidc_begin(&return_to).await
        }
        .map_err(|e| e.to_string())?;
        let pending = Pending {
            server,
            handle: start.handle,
            link,
        };
        let json = serde_json::to_string(&pending).map_err(|e| e.to_string())?;
        storage()
            .and_then(|s| s.set_item(KEY, &json).ok())
            .ok_or_else(|| t!("oidc-failed"))?;
        location.assign(&start.url).map_err(|_| t!("oidc-failed"))?;
        Ok(None)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::net::Ipv4Addr;
    use std::time::Duration;

    use starbooru_client::Client;
    use starbooru_core::OidcError;
    use starbooru_core::oidc::read_return;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::{Done, finish, return_error};

    /// How long the browser has to come back; the server forgets a
    /// sign-in after ten minutes anyway.
    const WAIT: Duration = Duration::from_secs(600);
    /// Longest request head read from the browser.
    const MAX_HEAD: usize = 8 * 1024;

    pub async fn run(client: Client, _server: String, link: bool) -> Result<Option<Done>, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let return_to = format!("http://127.0.0.1:{port}/");
        let start = if link {
            client.oidc_begin_link(&return_to).await
        } else {
            client.oidc_begin(&return_to).await
        }
        .map_err(|e| e.to_string())?;
        webbrowser::open(&start.url).map_err(|e| t!("oidc-no-browser", error = e.to_string()))?;
        let page = page(&t!("oidc-close-tab"));
        let code = tokio::time::timeout(WAIT, wait(&listener, &page))
            .await
            .map_err(|_| t!("oidc-expired"))?
            .map_err(return_error)?;
        finish(&client, link, code, start.handle).await.map(Some)
    }

    /// Answers the browser until it brings back a code or an error.
    pub(super) async fn wait(listener: &TcpListener, page: &str) -> Result<String, OidcError> {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return Err(OidcError::Failed);
            };
            if let Some(found) = answer(stream, page).await {
                return found;
            }
        }
    }

    async fn answer(mut stream: TcpStream, page: &str) -> Option<Result<String, OidcError>> {
        let head = tokio::time::timeout(Duration::from_secs(5), read_head(&mut stream))
            .await
            .unwrap_or_default();
        let found = request_query(&String::from_utf8_lossy(&head)).and_then(read_return);
        let (status, body) = match found {
            Some(_) => ("200 OK", page),
            None => ("404 Not Found", ""),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
             Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        found
    }

    async fn read_head(stream: &mut TcpStream) -> Vec<u8> {
        let mut head = Vec::new();
        let mut buf = [0u8; 1024];
        while head.len() < MAX_HEAD && !head.windows(4).any(|w| w == b"\r\n\r\n") {
            match stream.read(&mut buf).await {
                Ok(n) if n > 0 => head.extend_from_slice(buf.get(..n).unwrap_or_default()),
                _ => break,
            }
        }
        head
    }

    /// The query of a `GET /path?query HTTP/1.1` request head.
    pub(super) fn request_query(head: &str) -> Option<&str> {
        let mut line = head.lines().next()?.split(' ');
        if line.next()? != "GET" {
            return None;
        }
        line.next()?.split_once('?').map(|(_, q)| q)
    }

    /// The page the browser shows once it has handed over the answer.
    pub(super) fn page(text: &str) -> String {
        let text = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!(
            "<!doctype html><meta charset=utf-8><meta name=viewport \
             content=\"width=device-width\"><title>StarBooru</title>\
             <style>body{{font:16px system-ui,sans-serif;display:grid;place-items:center;\
             min-height:90vh;margin:0 16px;color-scheme:light dark}}</style><p>{text}</p>"
        )
    }
}

/// `/oidc`: where the web app comes back from the provider.
#[component]
pub fn OidcReturn(oidc_code: String, oidc_error: String) -> Element {
    let mut error = use_signal(|| None::<String>);
    let mut linking = use_signal(|| false);
    let nav = navigator();
    use_hook(move || {
        #[cfg(target_arch = "wasm32")]
        {
            // The router has the query; `location.search` may be gone by now.
            let found = if !oidc_code.is_empty() {
                Some(Ok(oidc_code))
            } else if !oidc_error.is_empty() {
                Some(Err(OidcError::parse(&oidc_error)))
            } else {
                None
            };
            let pending = platform::take();
            let (Some(found), Some(pending)) = (found, pending) else {
                nav.replace(crate::Route::Gallery { q: String::new() });
                return;
            };
            linking.set(pending.link);
            let code = match found {
                Ok(code) => code,
                Err(e) => {
                    error.set(Some(return_error(e)));
                    return;
                }
            };
            let client = if pending.link {
                crate::store::client()
            } else {
                Client::new(&pending.server).ok()
            };
            let Some(client) = client else {
                error.set(Some(t!("oidc-failed")));
                return;
            };
            spawn(async move {
                match finish(&client, pending.link, code, pending.handle).await {
                    Ok(Done::SignedIn(auth)) => {
                        crate::store::sign_in(pending.server, auth);
                        nav.replace(crate::Route::Gallery { q: String::new() });
                    }
                    Ok(Done::Linked(_)) => {
                        nav.replace(crate::Route::Settings {});
                    }
                    Err(e) => error.set(Some(e)),
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (&mut error, &mut linking, oidc_code, oidc_error);
            nav.replace(crate::Route::Gallery { q: String::new() });
        }
    });
    rsx! {
        div { class: "login-page",
            div { class: "aurora", aria_hidden: "true" }
            div { class: "login-card glass",
                match error() {
                    None => rsx! {
                        p { class: "muted", role: "status", {t!("oidc-finishing")} }
                    },
                    Some(message) => rsx! {
                        div { class: "banner error", role: "alert", "{message}" }
                        if linking() {
                            Link { class: "btn wide", to: crate::Route::Settings {}, {t!("oidc-back-settings")} }
                        } else {
                            Link { class: "btn wide", to: crate::Route::Gallery { q: String::new() }, {t!("oidc-back-sign-in")} }
                        }
                    },
                }
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use starbooru_core::OidcError;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::platform::{page, request_query, wait};

    async fn get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn waits_for_the_browser() {
        for (target, want) in [
            ("/?oidc_code=c0de&state=x", Ok("c0de".to_owned())),
            ("/?oidc_error=denied", Err(OidcError::Denied)),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let waiting = tokio::spawn(async move { wait(&listener, "<p>done</p>").await });
            let stray = get(port, "/favicon.ico").await;
            assert!(stray.starts_with("HTTP/1.1 404"), "{stray}");
            let answer = get(port, target).await;
            assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
            assert!(answer.ends_with("<p>done</p>"));
            assert!(answer.contains("Cache-Control: no-store"));
            assert_eq!(waiting.await.unwrap(), want);
        }
    }

    #[test]
    fn reads_the_browser_request() {
        assert_eq!(
            request_query("GET /?oidc_code=ab HTTP/1.1\r\nHost: x\r\n\r\n"),
            Some("oidc_code=ab")
        );
        assert_eq!(request_query("GET /favicon.ico HTTP/1.1\r\n"), None);
        assert_eq!(request_query("POST /?oidc_code=ab HTTP/1.1\r\n"), None);
        assert_eq!(request_query(""), None);
    }

    #[test]
    fn escapes_the_page() {
        let html = page("a <b> & c");
        assert!(html.contains("<p>a &lt;b&gt; &amp; c</p>"));
    }
}
