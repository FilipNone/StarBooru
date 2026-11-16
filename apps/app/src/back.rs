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
  // A sheet that is already closing has had its key: this one is for what's under it.
  const open = document.querySelector(".lightbox-btn.close, .select-backdrop, .sheet-scrim:not(.closing)");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheet_on_its_way_out_leaves_the_key_to_what_is_under_it() {
        // Regression: Back pressed while a sheet faded out clicked its scrim
        // again, which did nothing, and the key was used up.
        assert!(WATCH.contains(".sheet-scrim:not(.closing)"));
        let css = include_str!("../assets/main.css");
        let (_, closing) = css.split_once("\n.sheet-scrim.closing {").unwrap();
        let (closing, _) = closing.split_once('}').unwrap();
        assert!(closing.contains("pointer-events: none"));
    }
}
