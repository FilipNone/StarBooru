//! The picture that grows from its tile into the viewer, and shrinks back when
//! leaving. A copy of it floats above the page while the page underneath
//! changes. This is plain script rather than a view transition, as WebKitGTK
//! (the Linux desktop webview) aborts inside `startViewTransition`.

use dioxus::prelude::*;

/// The values are spelled out: the copy hangs off `<body>`, outside `.app` and
/// its motion tokens.
const HERO: &str = r##"
window.__sbHero ??= {
  ghost: null, hidden: null, stage: null, timer: 0, veil: 0, unwatch: null,
  calm() {
    return matchMedia("(prefers-reduced-motion: reduce)").matches
      || !!document.querySelector('.app[data-saver="on"]');
  },
  clear() {
    clearTimeout(this.timer);
    this.ghost?.remove();
    this.ghost = null;
    if (this.hidden) this.hidden.style.visibility = "";
    this.hidden = null;
    this.unveil();
    document.documentElement.classList.remove("sb-hero", "sb-hero-back");
  },
  unveil() {
    clearTimeout(this.veil);
    document.documentElement.classList.remove("sb-hero-leaving");
  },
  float(src, box, radius, under) {
    const ghost = document.createElement("img");
    ghost.className = "hero-ghost";
    ghost.alt = "";
    ghost.setAttribute("aria-hidden", "true");
    ghost.src = src;
    Object.assign(ghost.style, {
      position: "fixed", zIndex: "60", margin: "0", pointerEvents: "none", objectFit: "cover",
      left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px`,
      borderRadius: radius,
    });
    if (under) {
      ghost.style.background = `center / cover no-repeat url(${JSON.stringify(under)})`;
    }
    document.body.append(ghost);
    this.ghost = ghost;
    this.timer = setTimeout(() => this.clear(), 1500);
  },
  fly(box, radius) {
    const { left, top, width, height, borderRadius } = this.ghost.style;
    const flight = this.ghost.animate([
      { left, top, width, height, borderRadius },
      {
        left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px`,
        borderRadius: radius,
      },
    ], { duration: 420, easing: "cubic-bezier(0.2, 0, 0, 1)", fill: "forwards" });
    const end = () => this.clear();
    flight.finished.then(end, end);
  },
  open(id) {
    this.clear();
    const img = document.querySelector(`[data-post="${id}"] img`);
    if (this.calm() || !img?.complete || !img.naturalWidth) return;
    const tile = img.closest(".thumb");
    this.float(img.currentSrc || img.src, tile.getBoundingClientRect(), getComputedStyle(tile).borderRadius);
    document.documentElement.classList.add("sb-hero");
  },
  watch(el, id) {
    this.unwatch?.();
    const note = () => {
      if (!el.isConnected) return;
      const box = el.getBoundingClientRect();
      if (box.width < 2) return;
      this.stage = {
        id, at: 0, left: box.left, top: box.top, width: box.width, height: box.height,
        src: el.tagName === "IMG" ? el.currentSrc || el.src : "",
      };
    };
    note();
    const seen = new ResizeObserver(note);
    seen.observe(el);
    const settled = setTimeout(note, 500);
    addEventListener("scroll", note, { passive: true, capture: true });
    addEventListener("resize", note);
    el.addEventListener("load", note);
    this.unwatch = () => {
      seen.disconnect();
      clearTimeout(settled);
      removeEventListener("scroll", note, { capture: true });
      removeEventListener("resize", note);
      el.removeEventListener("load", note);
    };
  },
  arrive(id) {
    const el = document.querySelector("#stage-img, .stage video");
    if (!el) return;
    this.watch(el, id);
    const go = (tries) => {
      if (!this.ghost) return;
      scrollTo({ top: 0, behavior: "instant" });
      const box = el.getBoundingClientRect();
      if (box.width >= 2 && box.height >= 2) {
        el.style.visibility = "hidden";
        this.hidden = el;
        this.fly(box, "0px");
      } else if (tries > 0) {
        requestAnimationFrame(() => go(tries - 1));
      } else {
        this.clear();
      }
    };
    if (this.ghost && !this.hidden) go(30);
  },
  leave() {
    this.unwatch?.();
    this.unwatch = null;
    if (!this.stage) return;
    this.stage.at = performance.now();
    if (this.calm()) return;
    // The gallery stays out of sight until the picture lifts off, or it would
    // show finished for a moment first.
    document.documentElement.classList.add("sb-hero-leaving");
    clearTimeout(this.veil);
    this.veil = setTimeout(() => this.unveil(), 400);
  },
  back() {
    const from = this.stage;
    if (!from?.at || performance.now() - from.at > 1500) return;
    from.at = 0;
    if (this.calm()) return;
    const go = (tries) => {
      const tile = document.querySelector(`.masonry [data-post="${from.id}"]`)?.closest(".thumb");
      if (!tile) {
        if (tries > 0) requestAnimationFrame(() => go(tries - 1));
        else this.unveil();
        return;
      }
      const bar = document.querySelector(".topbar")?.getBoundingClientRect().bottom ?? 0;
      const nav = document.querySelector(".nav-bottom")?.getBoundingClientRect().top || innerHeight;
      let box = tile.getBoundingClientRect();
      // Mostly out of sight, after moving on in the viewer: bring it into view.
      if (Math.min(box.bottom, nav) - Math.max(box.top, bar) < box.height / 2) {
        scrollBy({ top: box.top - (innerHeight - box.height) / 2, behavior: "instant" });
        box = tile.getBoundingClientRect();
      }
      const media = tile.querySelector(".thumb-media");
      const thumb = media?.querySelector("img")?.currentSrc;
      const src = from.src || thumb;
      if (!media || !src) {
        this.unveil();
        return;
      }
      this.clear();
      this.float(src, from, "0px", thumb);
      media.style.visibility = "hidden";
      this.hidden = media;
      document.documentElement.classList.add("sb-hero-back");
      this.fly(box, getComputedStyle(tile).borderRadius);
    };
    go(10);
  },
};
"##;

/// Lifts a tile's picture off the grid. Returns once it floats, so the page can
/// change underneath it.
pub async fn open(id: i64) {
    let mut eval = document::eval(&format!(
        "{HERO}\nwindow.__sbHero.open({id});\ndioxus.send(true);"
    ));
    let _ = eval.recv::<bool>().await;
}

/// The viewer's picture is on the page: the floating one flies to it, and the
/// viewer keeps note of where it is for the way back.
pub fn arrive(id: i64) {
    document::eval(&format!("{HERO}\nwindow.__sbHero.arrive({id});"));
}

/// The viewer is going away.
pub fn leave() {
    document::eval(&format!("{HERO}\nwindow.__sbHero.leave();"));
}

/// The gallery is showing again: the picture just left flies back to its tile.
pub fn back() {
    document::eval(&format!("{HERO}\nwindow.__sbHero.back();"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_view_transitions() {
        // Regression: clicking a thumbnail crashed the Linux desktop app, as
        // WebKitGTK aborts its web process inside this call.
        assert!(!HERO.contains("startViewTransition"));
    }

    #[test]
    fn no_flying_while_motion_is_calmed() {
        for call in ["open(id) {", "leave() {", "back() {"] {
            let (_, body) = HERO.split_once(call).unwrap();
            let (body, _) = body.split_once("\n  },").unwrap();
            assert!(body.contains("this.calm()"), "{call}");
        }
    }
}
