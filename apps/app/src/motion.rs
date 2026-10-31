//! The picture that grows from its tile into the viewer, and shrinks back when
//! leaving. A copy of it floats above the page while the page underneath
//! changes. This is plain script rather than a view transition, as WebKitGTK
//! (the Linux desktop webview) aborts inside `startViewTransition`.

use dioxus::prelude::*;

/// The values are spelled out: the copy hangs off `<body>`, outside `.app` and
/// its motion tokens.
const HERO: &str = r##"
window.__sbHero ??= {
  ghost: null, hidden: null, flying: false, holding: false, past: null, stage: null, timer: 0, veil: 0, unwatch: null,
  calm() {
    return matchMedia("(prefers-reduced-motion: reduce)").matches
      || !!document.querySelector('.app[data-saver="on"]');
  },
  clear() {
    clearTimeout(this.timer);
    this.ghost?.remove();
    this.ghost = null;
    this.flying = false;
    this.holding = false;
    this.past?.remove();
    this.past = null;
    this.uncover();
    this.unveil();
    document.documentElement.classList.remove("sb-hero", "sb-hero-back");
  },
  uncover() {
    if (this.hidden) this.hidden.style.visibility = "";
    this.hidden = null;
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
  // The gallery goes the moment the viewer comes. A copy of it stays on top until
  // the viewer is there and fades out over it, or the page blinks empty between them.
  linger(id, tile) {
    const page = tile.closest(".page");
    const app = page?.closest(".app");
    if (!app) return;
    const box = page.getBoundingClientRect();
    const copy = page.cloneNode(true);
    for (const img of copy.querySelectorAll("img")) img.decoding = "sync";
    const media = copy.querySelector(`[data-post="${id}"]`)?.closest(".thumb")?.querySelector(".thumb-media");
    if (media) media.style.visibility = "hidden";
    Object.assign(copy.style, { left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px` });
    const past = document.createElement("div");
    past.className = "hero-past";
    past.setAttribute("aria-hidden", "true");
    past.style.setProperty("--pad-x", getComputedStyle(page).getPropertyValue("--pad-x"));
    past.append(copy);
    app.append(past);
    this.past = past;
  },
  fadePast() {
    const past = this.past;
    if (!past) return;
    const gone = () => {
      past.remove();
      if (this.past === past) this.past = null;
    };
    past.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 200, easing: "ease-out", fill: "forwards" })
      .finished.then(gone, gone);
  },
  // Landed: what it landed on shows underneath, and it leaves once that can
  // really paint. A new picture may still be coming or decoding, in which case
  // the thumbnail standing in for it takes over after a moment.
  async under(el) {
    document.documentElement.classList.remove("sb-hero");
    this.uncover();
    const shown = el?.tagName === "VIDEO" && el.readyState < 2
      ? new Promise((done) => el.addEventListener("loadeddata", done, { once: true }))
      : el?.decode?.().catch(() => {});
    await Promise.race([shown, new Promise((done) => setTimeout(done, 400))]);
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
  },
  // What it lands on can still move while the page settles, so it aims anew
  // each frame.
  fly(aim, radius, hold) {
    const ghost = this.ghost;
    const from = ghost.getBoundingClientRect();
    const round = parseFloat(ghost.style.borderRadius) || 0;
    const rounded = parseFloat(radius) || 0;
    this.flying = true;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.clear(), 1500);
    // Moves nothing itself: it keeps the time and the easing.
    const flight = ghost.animate([], { duration: 420, easing: "cubic-bezier(0.2, 0, 0, 1)", fill: "forwards" });
    const end = async () => {
      if (this.ghost !== ghost) return;
      this.holding = true;
      await hold();
      if (this.ghost === ghost) this.clear();
    };
    flight.finished.then(end, end);
    let to = from;
    const frame = () => {
      if (this.ghost !== ghost) return;
      to = aim() ?? to;
      const at = flight.effect.getComputedTiming().progress ?? 1;
      const mix = (a, b) => `${a + (b - a) * at}px`;
      Object.assign(ghost.style, {
        left: mix(from.left, to.left), top: mix(from.top, to.top),
        width: mix(from.width, to.width), height: mix(from.height, to.height),
        borderRadius: mix(round, rounded),
      });
      requestAnimationFrame(frame);
    };
    frame();
  },
  open(id) {
    // The page is about to go, and the last scroll may not be noted yet.
    window.__sbSpot?.report();
    this.clear();
    const img = document.querySelector(`[data-post="${id}"] img`);
    if (this.calm() || !img?.complete || !img.naturalWidth) return;
    const tile = img.closest(".thumb");
    this.float(img.currentSrc || img.src, tile.getBoundingClientRect(), getComputedStyle(tile).borderRadius);
    this.linger(id, tile);
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
        radius: getComputedStyle(el).borderRadius,
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
    // The flight is its entrance, and the gallery's fade the page's.
    if (this.ghost) {
      const page = el.closest(".page");
      for (const running of [...el.getAnimations(), ...(page?.getAnimations() ?? [])]) running.cancel();
    }
    this.watch(el, id);
    const go = (tries) => {
      if (!this.ghost || this.flying) return;
      scrollTo({ top: 0, behavior: "instant" });
      const box = el.getBoundingClientRect();
      if (box.width >= 2 && box.height >= 2) {
        this.fadePast();
        this.fly(() => (el.isConnected ? el.getBoundingClientRect() : null), getComputedStyle(el).borderRadius, () => this.under(el));
      } else if (tries > 0) {
        requestAnimationFrame(() => go(tries - 1));
      } else {
        this.clear();
      }
    };
    go(30);
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
  // The gallery only has tiles near the screen on the page, and the viewer may
  // have moved far from where it was: the tile's place comes from the layout.
  home(id, top, height) {
    const from = this.stage;
    const grid = document.querySelector(".masonry");
    if (!grid || from?.id !== id || !this.recent(from) || this.calm()) return;
    const bar = document.querySelector(".topbar")?.getBoundingClientRect().bottom ?? 0;
    const nav = document.querySelector(".nav-bottom")?.getBoundingClientRect().top || innerHeight;
    const start = grid.getBoundingClientRect().top + top;
    // Mostly out of sight: bring it into view.
    if (Math.min(start + height, nav) - Math.max(start, bar) < height / 2) {
      scrollBy({ top: start - (innerHeight - height) / 2, behavior: "instant" });
    }
  },
  recent(from) {
    return from?.at && performance.now() - from.at <= 1500;
  },
  // The tile a picture is flying back to, which may only now be on the page, or
  // be on it anew. It stays hidden until the picture lands on it.
  landing(id) {
    const tile = document.querySelector(`.masonry [data-post="${id}"]`)?.closest(".thumb");
    const media = tile?.querySelector(".thumb-media");
    if (!media) return null;
    if (!this.holding && media !== this.hidden) {
      this.uncover();
      media.style.visibility = "hidden";
      this.hidden = media;
    }
    for (const running of tile.getAnimations()) running.finish();
    return tile.getBoundingClientRect();
  },
  back() {
    const from = this.stage;
    if (!this.recent(from) || from.back) return;
    from.back = true;
    if (this.calm()) return;
    const go = (tries) => {
      const tile = document.querySelector(`.masonry [data-post="${from.id}"]`)?.closest(".thumb");
      if (!tile) {
        if (tries > 0) requestAnimationFrame(() => go(tries - 1));
        else this.unveil();
        return;
      }
      const thumb = tile.querySelector(".thumb-media img")?.currentSrc;
      const src = from.src || thumb;
      if (!src) {
        this.unveil();
        return;
      }
      this.clear();
      this.float(src, from, from.radius, thumb);
      document.documentElement.classList.add("sb-hero-back");
      this.fly(() => this.landing(from.id), getComputedStyle(tile).borderRadius, () => {
        const shown = this.hidden?.querySelector("img");
        return this.under(shown);
      });
    };
    go(30);
  },
};
"##;

/// The post last on the viewer's stage, the one a picture flies back from.
static SHOWN: GlobalSignal<Option<i64>> = Signal::global(|| None);

pub fn shown() -> Option<i64> {
    *SHOWN.peek()
}

/// How far the Upload tab's centre is from the middle of the window, where a
/// sheet sits. Nothing to measure means no distance.
pub async fn upload_tab() -> (f64, f64) {
    let mut eval = document::eval(
        r#"
        const tab = [...document.querySelectorAll(".nav-upload")]
          .map((el) => el.getBoundingClientRect())
          .find((box) => box.width > 0);
        dioxus.send(tab
          ? [tab.left + tab.width / 2 - innerWidth / 2, tab.top + tab.height / 2 - innerHeight / 2]
          : [0, 0]);
        "#,
    );
    eval.recv().await.unwrap_or_default()
}

/// Waits for the first element `selector` finds to finish animating. Call it
/// from an effect, so the animation has been asked for.
pub async fn settled(selector: &str) {
    let mut eval = document::eval(&format!(
        r#"
        await new Promise(requestAnimationFrame);
        const running = document.querySelector({selector:?})?.getAnimations() ?? [];
        await Promise.race([
          Promise.allSettled(running.map((a) => a.finished)),
          new Promise((done) => setTimeout(done, 800)),
        ]);
        dioxus.send(true);
        "#
    ));
    let _ = eval.recv::<bool>().await;
}

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
    *SHOWN.write() = Some(id);
    document::eval(&format!("{HERO}\nwindow.__sbHero.arrive({id});"));
}

/// The viewer is going away.
pub fn leave() {
    document::eval(&format!("{HERO}\nwindow.__sbHero.leave();"));
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
    fn a_flight_ending_starts_no_animation() {
        // Regression: a rule that changed an animation for as long as the
        // picture flew started that animation when it landed, and the whole
        // viewer faded in a second time.
        let css = include_str!("../assets/main.css");
        for rule in css.lines().filter(|l| l.starts_with("html.sb-hero ")) {
            assert!(!rule.contains("animation"), "{rule}");
        }
    }

    #[test]
    fn a_flight_follows_what_it_lands_on() {
        // Regression: the picture landed where its tile was at take-off, which
        // was wrong whenever the page moved after that.
        let (_, body) = HERO.split_once("fly(aim, radius, hold) {").unwrap();
        let (body, _) = body.split_once("\n  },").unwrap();
        assert!(body.contains("aim()") && body.contains("requestAnimationFrame"));
    }

    #[test]
    fn a_landed_picture_waits_for_the_real_one() {
        // Regression: it went at the end of the flight, while a new picture was
        // still coming or decoding.
        let (_, body) = HERO.split_once("async under(el) {").unwrap();
        let (body, _) = body.split_once("\n  },").unwrap();
        assert!(body.contains("decode") && body.contains("setTimeout"));
        let (_, body) = HERO.split_once("fly(aim, radius, hold) {").unwrap();
        assert!(body.contains("await hold()"));
    }

    #[test]
    fn the_gallery_fades_out_over_the_viewer() {
        // Regression: the page went empty between the gallery and the viewer.
        let (_, body) = HERO.split_once("  open(id) {").unwrap();
        let (body, _) = body.split_once("\n  },").unwrap();
        assert!(body.contains("this.linger(id, tile)"));
        let (_, body) = HERO.split_once("  arrive(id) {").unwrap();
        let (body, _) = body.split_once("\n  },").unwrap();
        assert!(body.contains("this.fadePast()"));
    }

    #[test]
    fn a_flight_keeps_the_pictures_corners() {
        // Regression: the picture flew to square corners and came back from
        // them, while the stage it sits on is rounded.
        assert!(!HERO.contains("\"0px\""));
        let css = include_str!("../assets/main.css");
        let (_, rule) = css.split_once(".stage img, .stage video {").unwrap();
        let (rule, _) = rule.split_once('}').unwrap();
        assert!(rule.contains("border-radius: inherit"));
    }

    #[test]
    fn opening_notes_where_the_gallery_is() {
        // Regression: a click within 200 ms of scrolling came back to an older spot.
        let (_, body) = HERO.split_once("open(id) {").unwrap();
        assert!(body.contains("__sbSpot?.report()"));
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
