//! The Android back key. The router keeps its history to itself here, so the
//! WebView has nothing to go back to and the key would close the app from any
//! page. So the activity (see `vendor/wry`) asks the page first: `__sbBack` says
//! whether it had a use for the key, and if not the key closes the app.
//!
//! What's open on top of the page closes first: a menu, a sheet, zoom.

use dioxus::prelude::*;

use crate::Route;

const WATCH: &str = r#"
let somewhere = false;
const close = () => {
  if (document.querySelector(".stage.full, .stage.zoomed")) {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    return true;
  }
  const open = document.querySelector(".lightbox-btn.close, .select-backdrop, .sheet-scrim");
  open?.click();
  return !!open;
};
window.__sbBack = () => {
  if (close()) return true;
  if (somewhere) dioxus.send(true);
  return somewhere;
};
while (true) somewhere = await dioxus.recv();
"#;

/// Answers the back key. Call once, from the layout, with the current route.
pub fn watch(route: &Route) {
    let page = use_hook(|| {
        let page = document::eval(WATCH);
        let mut events = page;
        spawn(async move {
            while events.recv::<bool>().await.is_ok() {
                navigator().go_back();
            }
        });
        page
    });
    use_effect(use_reactive((route,), move |_| {
        let _ = page.send(navigator().can_go_back());
    }));
    use_drop(|| {
        document::eval("window.__sbBack = null;");
    });
}
