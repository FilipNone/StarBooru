//! Taps the browser drops. On Android a quick swipe leaves the browser sure the
//! page is still coasting, even when the swipe was the app's own and nothing
//! scrolled: a post turned or closed, the editor's dial dragged. For a few
//! hundred milliseconds after it, a tap only stops that scroll, and its click
//! never comes. The same goes for a tap on the arrow to the top while the
//! gallery really is coasting.
//!
//! The page watches each tap. One that gets no click of its own soon after it
//! gets one from the page, unless it stopped a scroll that carried what it
//! landed on: that tap was to stop the gallery, as in any list on a phone.
//! What stays in place while the page moves (the bars, the arrow) takes the
//! tap anyway.

use dioxus::prelude::*;

pub const SCRIPT: &str = r#"
(() => {
  if (window.__sbTaps) return;
  window.__sbTaps = true;
  // Under the platform's long press, which opens a menu instead.
  const SLOP = 12, LONG = 380, WAIT = 150, MOVING = 100, NEAR = 32;
  // The tap under way, the one still owed a click, and where one was given.
  let tap = null, owed = null, given = null;
  // The last scroll events, of the page or of a box in it.
  const recent = [];
  addEventListener("scroll", (e) => {
    recent.push([e.timeStamp, e.target === document ? document.scrollingElement : e.target]);
    if (recent.length > 16) recent.shift();
  }, { capture: true, passive: true });
  // What was scrolling as the finger came down. A scroll moves every frame,
  // where the app's own jumps come one at a time.
  const moving = (at) => {
    const count = new Map();
    for (const [t, box] of recent) if (at - t <= MOVING) count.set(box, (count.get(box) ?? 0) + 1);
    return [...count].filter(([, n]) => n >= 3).map(([box]) => box);
  };
  // Whether `el` moves when `box` scrolls.
  const carried = (el, box) => {
    if (!box?.contains(el)) return false;
    for (let at = el; at && at !== box; at = at.parentElement) {
      const place = getComputedStyle(at).position;
      if (place === "fixed" || place === "sticky") return false;
    }
    return true;
  };
  // A field takes its focus and keyboard from the tap itself.
  const typed = (el) => !!el.closest(
    'input:not([type=button], [type=submit], [type=reset], [type=checkbox], [type=radio], [type=file], [type=color]), textarea, select, [contenteditable]:not([contenteditable="false"])',
  );
  const near = (e, at) => Math.hypot(e.clientX - at.x, e.clientY - at.y) <= NEAR;
  const drop = () => {
    if (owed) clearTimeout(owed.timer);
    owed = null;
  };
  const give = () => {
    const o = owed;
    drop();
    if (!o?.el.isConnected) return;
    given = { x: o.x, y: o.y };
    o.el.dispatchEvent(new MouseEvent("click", {
      bubbles: true, cancelable: true, composed: true, view: window, detail: 1, button: 0,
      clientX: o.x, clientY: o.y, screenX: o.sx, screenY: o.sy,
    }));
  };
  addEventListener("pointerdown", (e) => {
    // The click of the tap before comes ahead of the next touch, so a tap
    // still owed one has lost it.
    give();
    given = null;
    tap = null;
    if (e.pointerType === "mouse" || !e.isPrimary || !(e.target instanceof Element)) return;
    tap = { id: e.pointerId, x: e.clientX, y: e.clientY, t: e.timeStamp, el: e.target, stopped: moving(e.timeStamp) };
  }, { capture: true, passive: true });
  addEventListener("pointermove", (e) => {
    if (tap && e.pointerId === tap.id && Math.hypot(e.clientX - tap.x, e.clientY - tap.y) > SLOP) tap = null;
  }, { capture: true, passive: true });
  for (const kind of ["pointercancel", "contextmenu"]) {
    addEventListener(kind, () => {
      tap = null;
      drop();
    }, { capture: true, passive: true });
  }
  addEventListener("pointerup", (e) => {
    const t = tap;
    tap = null;
    if (!t || e.pointerId !== t.id || e.timeStamp - t.t > LONG) return;
    if (Math.hypot(e.clientX - t.x, e.clientY - t.y) > SLOP || typed(t.el)) return;
    if (t.stopped.some((box) => carried(t.el, box))) return;
    owed = { x: e.clientX, y: e.clientY, sx: e.screenX, sy: e.screenY, el: t.el, timer: setTimeout(give, WAIT) };
  }, { capture: true, passive: true });
  // Something on the page wanted no click.
  addEventListener("touchend", (e) => {
    if (e.defaultPrevented) drop();
  }, { passive: true });
  addEventListener("click", (e) => {
    if (!e.isTrusted) return;
    if (owed && near(e, owed)) drop();
    else if (given && near(e, given)) {
      // The tap's own click, later than the one given for it.
      given = null;
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  }, { capture: true });
})();
"#;

/// Puts the script in place. Call once, from the root component.
pub fn install() {
    use_hook(|| {
        document::eval(SCRIPT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(start: &str) -> &'static str {
        let (_, rest) = SCRIPT.split_once(start).unwrap();
        rest.split_once("\n  }").map_or(rest, |(body, _)| body)
    }

    #[test]
    fn a_tap_gets_a_click_only_when_its_own_never_came() {
        // Regression: on a phone, a tap soon after a swipe (closing a post,
        // turning one, the editor's dial) or on the arrow while the gallery
        // coasted had no click, and the button did nothing.
        let up = part(r#"addEventListener("pointerup", (e) => {"#);
        assert!(up.contains("timer: setTimeout(give, WAIT)"));
        let click = part(r#"addEventListener("click", (e) => {"#);
        assert!(click.contains("if (!e.isTrusted) return;"));
        assert!(click.contains("if (owed && near(e, owed)) drop();"));
        // One that comes late, after the page gave one, goes no further.
        assert!(click.contains("e.stopImmediatePropagation();"));
        // Before every other listener on the window.
        assert!(SCRIPT.contains("}, { capture: true });\n})();"));
        // The click of the tap before always comes ahead of the next touch.
        let down = part(r#"addEventListener("pointerdown", (e) => {"#);
        assert!(down.trim_start().contains("give();\n    given = null;"));
    }

    #[test]
    fn a_tap_that_stops_a_scroll_stays_a_stop() {
        // A tile under a finger that stops the gallery opens nothing, as in
        // any list. What doesn't move with the scroll still takes the tap.
        let down = part(r#"addEventListener("pointerdown", (e) => {"#);
        assert!(down.contains("stopped: moving(e.timeStamp)"));
        assert!(SCRIPT.contains("if (t.stopped.some((box) => carried(t.el, box))) return;"));
        let carried = part("const carried = (el, box) => {");
        assert!(carried.contains(r#"place === "fixed" || place === "sticky""#));
        // A scroll, not the app's own jumps: those come one frame at a time.
        assert!(SCRIPT.contains("filter(([, n]) => n >= 3)"));
    }

    #[test]
    fn only_a_plain_tap_is_given_a_click() {
        let up = part(r#"addEventListener("pointerup", (e) => {"#);
        assert!(up.contains("e.timeStamp - t.t > LONG"));
        assert!(up.contains("> SLOP || typed(t.el)"));
        let down = part(r#"addEventListener("pointerdown", (e) => {"#);
        assert!(down.contains(r#"e.pointerType === "mouse" || !e.isPrimary"#));
        // A long press, a second finger, a scroll the browser takes, or a page
        // that wanted no click.
        assert!(SCRIPT.contains(r#"for (const kind of ["pointercancel", "contextmenu"])"#));
        assert!(SCRIPT.contains("if (e.defaultPrevented) drop();"));
        // Under the platform's long press (400 ms) and gesture.rs's hold.
        let (_, long) = SCRIPT.split_once("LONG = ").unwrap();
        let long: u32 = long.split_once(',').unwrap().0.parse().unwrap();
        assert!(long < 400);
        let (_, hold) = include_str!("gesture.rs").split_once("HOLD = ").unwrap();
        let hold: u32 = hold.split_once(',').unwrap().0.parse().unwrap();
        assert!(long < hold);
    }

    #[test]
    fn it_is_on_every_page() {
        let main = include_str!("main.rs");
        assert!(main.contains("taps::install();"));
    }
}
