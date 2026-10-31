# wry 0.53.5, patched

This is [wry](https://github.com/tauri-apps/wry) 0.53.5 from crates.io (Apache-2.0 or MIT, see `LICENSE-APACHE` and `LICENSE-MIT`) with one change, to custom protocol requests on Android. The diff is `android-custom-protocol.patch`. The examples were left out.

Upstream, `handle_request` keeps the `REQUEST_HANDLER` lock while it waits for a response, and panics after 10 seconds without one, in a function that can't unwind. So one request at a time gets through, and:

1. The WebView asks for `/sbcache/…`, which downloads a picture.
2. A tap sends a synchronous `/__events` request, which waits for the lock. JavaScript stops until it gets an answer.
3. Dioxus won't poll tasks until JavaScript has applied the last render, so the download never finishes.
4. After 10 seconds the whole app aborts. Any download over 10 seconds does the same.

Here the lock covers only the call to the handler, and a response that takes over `CUSTOM_PROTOCOL_TIMEOUT` makes the request fail. A response after that is dropped.

wry 0.57 stops waiting under the lock and doesn't panic either, but it runs the handler outside the lock as well. Dioxus 0.7's handler isn't thread safe (`RefCell`s in `assets.rs`), so the handler stays under the lock here.

The root `Cargo.toml` points crates.io's `wry` here with `[patch.crates-io]`. Remove this folder and that entry once `dioxus-desktop` depends on a wry with this fixed and its own handler copes with running at the same time on several threads.
