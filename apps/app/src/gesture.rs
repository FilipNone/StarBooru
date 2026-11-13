//! Holding, right-clicking and swiping things in lists: a thumbnail or a tag
//! opens its menu when held or right-clicked, and a thumbnail swiped sideways
//! does what the settings say (`SwipeAction`). The viewer's own swipes are in
//! `pages/post.rs`.
//!
//! What follows the finger is moved by the page's script, frame by frame,
//! without a trip through the app: the app hears once, when the gesture is
//! over, what it was for.

use std::sync::atomic::{AtomicU32, Ordering};

use dioxus::prelude::*;
use serde::Deserialize;
use starbooru_core::normalize_tag;

/// What a gesture was made on, from the `data-press` attribute it was made
/// under: `post:<id>` on a thumbnail, or `tag` on a list of tag links, which
/// the script turns into `tag:<name>` with the name of the one held.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Post(i64),
    Tag(String),
}

impl Target {
    /// `None` for anything that isn't a post id or a valid tag name.
    pub fn parse(key: &str) -> Option<Target> {
        let (kind, rest) = key.split_once(':')?;
        match kind {
            "post" => rest
                .parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .map(Target::Post),
            "tag" => normalize_tag(rest).ok().map(Target::Tag),
            _ => None,
        }
    }
}

/// The side a thumbnail was swiped to: `Left` when the finger went left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

/// Where a menu was asked for, and the size of the window then, in CSS
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuAt {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A menu's width, unless the window is narrower.
pub const MENU_W: f64 = 248.0;
/// Space kept between a menu and the window's edges.
const EDGE: f64 = 8.0;

impl MenuAt {
    /// The menu's place: its corner at the point, on the side of it with more
    /// room, and never past the window's edges. Its animation grows from the
    /// point.
    pub fn style(self) -> String {
        let sane = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let (vw, vh) = (sane(self.width), sane(self.height));
        let (x, y) = (sane(self.x).min(vw), sane(self.y).min(vh));
        let w = MENU_W.min(vw - 2.0 * EDGE).max(0.0);
        let right_room = vw - x - EDGE;
        let left = if right_room >= w { x } else { x - w };
        let left = left.min(vw - w - EDGE).max(EDGE.min(vw - w).max(0.0));
        let origin_x = x - left;
        if y > vh / 2.0 {
            let bottom = vh - y;
            let room = (y - EDGE).max(0.0);
            format!(
                "left: {left:.0}px; bottom: {bottom:.0}px; width: {w:.0}px; max-height: {room:.0}px; transform-origin: {origin_x:.0}px 100%"
            )
        } else {
            let room = (vh - y - EDGE).max(0.0);
            format!(
                "left: {left:.0}px; top: {y:.0}px; width: {w:.0}px; max-height: {room:.0}px; transform-origin: {origin_x:.0}px 0"
            )
        }
    }
}

/// A gesture that came to something.
#[derive(Clone, Debug, PartialEq)]
pub enum Press {
    /// Held, right-clicked, or the menu key pressed on it.
    Menu(Target, MenuAt),
    /// Swiped far enough to do the side's action.
    Swipe(Target, Side),
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Raw {
    Menu {
        key: String,
        x: f64,
        y: f64,
        vw: f64,
        vh: f64,
    },
    Swipe {
        key: String,
        side: Side,
    },
}

impl Press {
    /// What the script sent, or `None` if it makes no sense.
    pub fn parse(value: serde_json::Value) -> Option<Press> {
        match serde_json::from_value::<Raw>(value).ok()? {
            Raw::Menu { key, x, y, vw, vh } => Some(Press::Menu(
                Target::parse(&key)?,
                MenuAt {
                    x,
                    y,
                    width: vw,
                    height: vh,
                },
            )),
            Raw::Swipe { key, side } => Some(Press::Swipe(Target::parse(&key)?, side)),
        }
    }
}

/// Listens for holds, right clicks and swipes on whatever is marked
/// `data-press="<kind>…"`, for as long as the component is there. Each kind has
/// one listener at a time: the newest takes it over, and the one before can't
/// stop it on its way out. A page can listen for both kinds, as the viewer
/// does for its tags and its related posts.
pub fn use_press(kind: &'static str, on: Callback<Press>) {
    let token = use_hook(|| {
        static NEXT: AtomicU32 = AtomicU32::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    });
    use_future(move || async move {
        let mut eval = document::eval(&format!(
            "{PRESS}\nwindow.__sbPress.start({token}, {kind:?}, (m) => dioxus.send(m));"
        ));
        while let Ok(value) = eval.recv::<serde_json::Value>().await {
            if let Some(press) = Press::parse(value) {
                on.call(press);
            }
        }
    });
    use_drop(move || {
        document::eval(&format!("window.__sbPress?.stop({token});"));
    });
}

/// The page's side of [`use_press`].
///
/// A finger held still on a target for `HOLD` ms opens its menu, and the target
/// sinks a little meanwhile (after a short wait, so a finger that only starts
/// a scroll doesn't make it twitch), then comes back from wherever it got to.
/// A right click, or the menu key, opens it at once. Android also sends a
/// `contextmenu` for a long press, which is dropped if the menu has just
/// opened.
///
/// A thumbnail with a `.swipe-hint` for a side can be swiped that way. Its
/// picture follows the finger (or two fingers on a touchpad) and shows the
/// action under it; let go past `reach`, or flicked, the action runs and the
/// picture slides back. A side without an action gives a little and no more.
///
/// The click that follows a hold or a swipe is dropped, as it would open the
/// post or close the menu that just opened.
const PRESS: &str = r##"
window.__sbPress ??= (() => {
  const HOLD = 450, SLOP = 10, EASE = "cubic-bezier(0.2, 0, 0, 1)";
  // Who listens for each kind: its token and where to send.
  const self = { kinds: new Map() };
  let press = null, shown = -1e9, swallow = -1e9, wheel = { quiet: -1e9, p: null };

  const find = (e) => {
    if (!self.kinds.size || !(e.target instanceof Element)) return null;
    const host = e.target.closest("[data-press]");
    if (!host) return null;
    const value = host.dataset.press;
    const at = value.indexOf(":");
    const kind = at < 0 ? value : value.slice(0, at);
    const send = self.kinds.get(kind)?.send;
    if (!send) return null;
    if (at >= 0) return { el: host, key: value, send };
    const link = e.target.closest("a");
    if (!link || !host.contains(link)) return null;
    const name = (link.querySelector(".tag-name") ?? link).textContent.trim();
    return name ? { el: link, key: `${kind}:${name}`, send } : null;
  };
  const sidesOf = (el) => new Set([...el.querySelectorAll(":scope > .swipe-hint")].map((h) => h.dataset.side));
  const reach = (el) => Math.min(120, Math.max(56, el.clientWidth * 0.4));
  const open = (t, x, y) => {
    const now = performance.now();
    if (now - shown < 800) return;
    shown = now;
    t.send({ kind: "menu", key: t.key, x, y, vw: innerWidth, vh: innerHeight });
  };
  const calm = () => matchMedia("(prefers-reduced-motion: reduce)").matches
    || !!document.querySelector('.app[data-saver="on"]');
  // Held, it sinks. Its values are spelled out, as the script has no use for tokens.
  const sink = (p) => {
    if (!calm()) p.sunk = p.el.animate([{ scale: 1 }, { scale: 0.94 }], { duration: 350, delay: 100, easing: EASE, fill: "forwards" });
  };
  // Let go, it comes back from as far as it sank.
  const rise = (p) => {
    const sunk = p.sunk;
    if (!sunk) return;
    p.sunk = null;
    const from = getComputedStyle(p.el).scale;
    sunk.cancel();
    if (from && from !== "none" && from !== "1") p.el.animate([{ scale: from }, { scale: 1 }], { duration: 180, easing: EASE });
  };

  // The picture under the finger, `dx` from its place.
  const drag = (p, dx) => {
    const side = dx < 0 ? "left" : "right";
    const can = p.sides.has(side);
    const moved = can ? dx : dx * 0.15;
    p.el.dataset.swiping = side;
    p.el.toggleAttribute("data-armed", can && Math.abs(dx) >= reach(p.el));
    if (p.media) p.media.style.translate = `${moved}px 0`;
  };
  // Back to its place, and the action if it went far enough.
  const settle = (p, side) => {
    const { el, media } = p;
    el.dataset.swiping = "back";
    el.removeAttribute("data-armed");
    if (media) media.style.translate = "";
    clearTimeout(p.back);
    setTimeout(() => {
      if (el.dataset.swiping === "back") delete el.dataset.swiping;
    }, 320);
    if (side) {
      navigator.vibrate?.(8);
      p.send({ kind: "swipe", key: p.key, side });
    }
  };
  const speed = (trail) => {
    const last = trail[trail.length - 1];
    const first = trail.find(([t]) => last[0] - t <= 100) ?? last;
    return last[0] > first[0] ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
  };

  const cancel = () => {
    const p = press;
    press = null;
    if (!p) return;
    clearTimeout(p.timer);
    rise(p);
    if (p.axis === "x") settle(p, null);
  };
  const down = (e) => {
    // A second finger: a pinch or a scroll, not a hold.
    if (press) {
      cancel();
      if (!e.isPrimary) return;
    }
    if (e.pointerType === "mouse" || !e.isPrimary) return;
    const t = find(e);
    if (!t) return;
    const p = {
      id: e.pointerId, key: t.key, el: t.el, send: t.send, x0: e.clientX, y0: e.clientY, dx: 0, axis: null,
      trail: [[e.timeStamp, e.clientX]], sides: sidesOf(t.el), media: t.el.querySelector(":scope > .thumb-media"),
    };
    sink(p);
    p.timer = setTimeout(() => {
      if (press !== p || p.axis) return;
      rise(p);
      p.held = true;
      navigator.vibrate?.(12);
      open(t, p.x0, p.y0);
    }, HOLD);
    press = p;
  };
  const move = (e) => {
    const p = press;
    if (!p || e.pointerId !== p.id || p.held) return;
    const dx = e.clientX - p.x0, dy = e.clientY - p.y0;
    p.trail.push([e.timeStamp, e.clientX]);
    if (p.trail.length > 8) p.trail.shift();
    if (!p.axis) {
      if (Math.hypot(dx, dy) < SLOP) return;
      clearTimeout(p.timer);
      rise(p);
      // Up or down is a scroll, and the browser's.
      if (!p.sides.size || Math.abs(dx) < Math.abs(dy) * 1.2) {
        press = null;
        return;
      }
      p.axis = "x";
    }
    p.dx = dx;
    drag(p, dx);
  };
  const up = (e) => {
    const p = press;
    if (!p || e.pointerId !== p.id) return;
    press = null;
    clearTimeout(p.timer);
    rise(p);
    if (p.held || p.axis) swallow = e.timeStamp + 500;
    if (p.axis !== "x") return;
    const side = p.dx < 0 ? "left" : "right";
    const v = speed(p.trail);
    const flung = Math.abs(v) > 0.5 && Math.sign(v) === Math.sign(p.dx) && Math.abs(p.dx) > 24;
    settle(p, p.sides.has(side) && (Math.abs(p.dx) >= reach(p.el) || flung) ? side : null);
  };
  const lost = (e) => {
    if (press && e.pointerId === press.id) cancel();
  };
  const click = (e) => {
    // Only a real one: a menu's download clicks a link of its own.
    if (e.isTrusted && e.timeStamp < swallow) {
      e.preventDefault();
      e.stopPropagation();
      swallow = -1e9;
    }
  };
  const context = (e) => {
    const t = find(e);
    if (!t) return;
    e.preventDefault();
    if (press?.held) return;
    cancel();
    let { clientX: x, clientY: y } = e;
    // From the keyboard: at the middle of what has the focus.
    if (!x && !y) {
      const r = t.el.getBoundingClientRect();
      x = r.left + r.width / 2;
      y = r.top + r.height / 2;
    }
    open(t, x, y);
  };
  // Two fingers sideways on a touchpad. A mouse wheel's notch is bigger, and
  // isn't a swipe.
  const roll = (e) => {
    if (e.timeStamp > wheel.quiet) {
      wheel.p = null;
      const ax = Math.abs(e.deltaX), ay = Math.abs(e.deltaY);
      if (!e.ctrlKey && e.deltaMode === 0 && ax > ay && ax < 40) {
        const t = find(e);
        const sides = t ? sidesOf(t.el) : new Set();
        if (t && sides.size) {
          wheel.p = { key: t.key, el: t.el, send: t.send, sides, media: t.el.querySelector(":scope > .thumb-media"), dx: 0, done: false };
        }
      }
    }
    wheel.quiet = e.timeStamp + 300;
    const p = wheel.p;
    if (!p || p.done) return;
    clearTimeout(p.back);
    p.dx -= e.deltaX;
    const side = p.dx < 0 ? "left" : "right";
    if (p.sides.has(side) && Math.abs(p.dx) >= reach(p.el)) {
      p.done = true;
      settle(p, side);
      return;
    }
    drag(p, p.dx);
    p.back = setTimeout(() => {
      if (!p.done) settle(p, null);
    }, 160);
  };

  const on = [
    ["pointerdown", down], ["pointermove", move], ["pointerup", up], ["pointercancel", lost],
    ["click", click, true], ["contextmenu", context], ["wheel", roll],
  ];
  self.start = (token, kind, send) => {
    if (!self.kinds.size) {
      for (const [type, fn, capture] of on) addEventListener(type, fn, { capture: !!capture, passive: type !== "click" && type !== "contextmenu" });
    }
    self.kinds.set(kind, { token, send });
  };
  self.stop = (token) => {
    let mine = false;
    for (const [kind, by] of self.kinds) {
      if (by.token === token) mine = self.kinds.delete(kind);
    }
    if (!mine) return;
    cancel();
    if (self.kinds.size) return;
    for (const [type, fn, capture] of on) removeEventListener(type, fn, { capture: !!capture });
  };
  return self;
})();
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_what_a_gesture_was_on() {
        assert_eq!(Target::parse("post:42"), Some(Target::Post(42)));
        assert_eq!(
            Target::parse("tag:blue_sky"),
            Some(Target::Tag("blue_sky".into()))
        );
        // A name with a colon of its own keeps it.
        assert_eq!(
            Target::parse("tag:re:zero"),
            Some(Target::Tag("re:zero".into()))
        );
        // As the chip shows it, in any case.
        assert_eq!(
            Target::parse("tag:  Blue Sky "),
            Some(Target::Tag("blue_sky".into()))
        );
        for bad in [
            "",
            "post",
            "post:",
            "post:0",
            "post:-3",
            "post:1e9",
            "post:99999999999999999999",
            "tag:",
            "tag:   ",
            "tag:-cloud",
            "user:1",
        ] {
            assert_eq!(Target::parse(bad), None, "{bad}");
        }
        let long = format!("tag:{}", "a".repeat(10_000));
        assert_eq!(Target::parse(&long), None);
    }

    #[test]
    fn reads_what_the_script_sends() {
        let menu = json!({"kind": "menu", "key": "post:7", "x": 10.0, "y": 20.0, "vw": 400.0, "vh": 800.0});
        assert_eq!(
            Press::parse(menu),
            Some(Press::Menu(
                Target::Post(7),
                MenuAt {
                    x: 10.0,
                    y: 20.0,
                    width: 400.0,
                    height: 800.0
                }
            ))
        );
        let swipe = json!({"kind": "swipe", "key": "post:7", "side": "left"});
        assert_eq!(
            Press::parse(swipe),
            Some(Press::Swipe(Target::Post(7), Side::Left))
        );
        for bad in [
            json!(null),
            json!("menu"),
            json!({"kind": "menu", "key": "post:7"}),
            json!({"kind": "swipe", "key": "post:7", "side": "up"}),
            json!({"kind": "swipe", "key": "post:nope", "side": "left"}),
            json!({"kind": "fling", "key": "post:7"}),
        ] {
            assert_eq!(Press::parse(bad.clone()), None, "{bad}");
        }
    }

    fn px(style: &str, name: &str) -> f64 {
        style
            .split("; ")
            .find_map(|part| part.strip_prefix(&format!("{name}: ")))
            .and_then(|v| v.strip_suffix("px"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(f64::NAN)
    }

    fn at(x: f64, y: f64) -> MenuAt {
        MenuAt {
            x,
            y,
            width: 400.0,
            height: 800.0,
        }
    }

    #[test]
    fn a_menu_opens_beside_the_point_on_the_side_with_room() {
        // Top left: down and to the right, from the point.
        let s = at(20.0, 100.0).style();
        assert_eq!((px(&s, "left"), px(&s, "top")), (20.0, 100.0));
        assert!(s.contains("transform-origin: 0px 0"));
        // Near the right edge: to the left of the point.
        let s = at(380.0, 100.0).style();
        assert_eq!(px(&s, "left"), 380.0 - MENU_W);
        assert!(s.contains(&format!("transform-origin: {MENU_W:.0}px 0")));
        // Low down: upwards, its bottom at the point.
        let s = at(20.0, 700.0).style();
        assert_eq!(px(&s, "bottom"), 100.0);
        assert_eq!(px(&s, "max-height"), 692.0);
        assert!(s.contains("100%"));
    }

    #[test]
    fn a_menu_stays_inside_the_window() {
        // Narrower than the menu: as wide as the window allows.
        let narrow = MenuAt {
            x: 150.0,
            y: 10.0,
            width: 200.0,
            height: 400.0,
        };
        let s = narrow.style();
        assert_eq!((px(&s, "left"), px(&s, "width")), (8.0, 184.0));
        // Off the window, or nonsense: at an edge, not off the page.
        for odd in [
            at(-50.0, -50.0),
            at(f64::NAN, f64::INFINITY),
            at(1e12, 1e12),
            MenuAt {
                x: 1.0,
                y: 1.0,
                width: f64::NAN,
                height: -1.0,
            },
        ] {
            let s = odd.style();
            for name in ["left", "width", "max-height"] {
                let v = px(&s, name);
                assert!(v.is_finite() && v >= 0.0, "{name} in {s}");
            }
            let left = px(&s, "left");
            assert!(
                left + px(&s, "width") <= odd.width.max(0.0).max(MENU_W),
                "{s}"
            );
        }
    }

    #[test]
    fn each_kind_has_a_listener_of_its_own() {
        // Regression: one page listened for one kind, so the viewer's tags
        // had a menu and its related posts none.
        assert!(PRESS.contains("self.kinds.set(kind, { token, send });"));
        assert!(PRESS.contains("const send = self.kinds.get(kind)?.send;"));
        // A page on its way out only stops what is still its own.
        let (_, stop) = PRESS.split_once("self.stop = (token) => {").unwrap();
        let (stop, _) = stop.split_once("\n  };").unwrap();
        assert!(stop.contains("if (by.token === token) mine = self.kinds.delete(kind);"));
        assert!(stop.contains("if (!mine) return;"));
        // The page's listeners stay while any kind is listened for.
        let kept = stop.find("if (self.kinds.size) return;").unwrap();
        assert!(kept < stop.find("removeEventListener(").unwrap());
        // What a gesture is sent to is settled when it starts.
        assert!(PRESS.contains("p.send({ kind: \"swipe\", key: p.key, side });"));
        assert!(PRESS.contains(
            "t.send({ kind: \"menu\", key: t.key, x, y, vw: innerWidth, vh: innerHeight });"
        ));
    }

    #[test]
    fn a_held_thing_comes_back_from_where_it_sank() {
        // Regression: let go, it snapped back to its size.
        let (_, rise) = PRESS.split_once("const rise = (p) => {").unwrap();
        let (rise, _) = rise.split_once("\n  };").unwrap();
        let read = rise.find("getComputedStyle(p.el).scale").unwrap();
        assert!(read < rise.find("sunk.cancel();").unwrap());
        assert!(rise.contains("{ scale: from }, { scale: 1 }"));
        let (_, sink) = PRESS.split_once("const sink = (p) => {").unwrap();
        let (sink, _) = sink.split_once("\n  };").unwrap();
        assert!(sink.contains("if (!calm())"));
        // Every way a press ends lets it up.
        assert_eq!(PRESS.matches("rise(p);").count(), 4);
    }
}
