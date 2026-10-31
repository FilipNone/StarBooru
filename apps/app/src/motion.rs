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
    const dock = document.querySelector(".nav-bottom")?.getBoundingClientRect();
    // At the side it takes no height from the screen.
    const nav = dock?.width > dock?.height ? dock.top : innerHeight;
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

/// Going to the previous or next post. A big picture takes a moment to decode,
/// and an entrance started before it can paint is half over when it shows, on
/// an empty stage. So a copy of the old picture stays while the new one gets
/// ready, out of sight, and then both move at once.
const TURN: &str = r##"
window.__sbTurn ??= {
  past: null, since: 0, timer: 0,
  async hold() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.end(), 1200);
    // Nothing new has shown yet: what's held stays.
    if (this.since) return;
    const stage = document.querySelector(".stage");
    if (!stage) return;
    this.past?.remove();
    this.past = null;
    this.since = performance.now();
    delete stage.dataset.turning;
    if (!stage.classList.contains("full")) stage.style.height = `${stage.offsetHeight}px`;
    const el = document.getElementById("stage-img");
    if (!el) return;
    const copy = el.cloneNode(false);
    copy.removeAttribute("id");
    copy.removeAttribute("data-dioxus-id");
    copy.removeAttribute("data-turn");
    copy.className = el.classList.contains("loading") ? "stage-past loading" : "stage-past";
    copy.alt = "";
    copy.ariaHidden = "true";
    copy.decoding = "sync";
    const [width, height] = [el.offsetWidth, el.offsetHeight];
    // From the middle, where a picture of any shape stands.
    copy.style.width = `${width}px`;
    copy.style.height = `${height}px`;
    copy.style.left = `calc(50% - ${width / 2}px)`;
    copy.style.top = `calc(50% - ${height / 2}px)`;
    stage.prepend(copy);
    this.past = copy;
    // Paintable before the old picture goes.
    await Promise.race([copy.decode().catch(() => {}), new Promise((done) => setTimeout(done, 60))]);
  },
  show(thumb) {
    const el = document.querySelector('.stage [data-turn="wait"]');
    if (!el) return;
    const left = Math.max(0, 300 - (performance.now() - this.since));
    const after = (ms) => new Promise((done) => setTimeout(done, ms));
    const ready = el.tagName === "VIDEO"
      ? new Promise((done) => {
          if (el.readyState >= 2) done();
          else el.addEventListener("loadeddata", done, { once: true });
        })
      : el.decode().catch(() => {});
    // A slow one starts sooner if its thumbnail can stand in.
    const standIn = new Image();
    if (thumb) standIn.src = thumb;
    const stood = thumb
      ? standIn.decode().then(() => after(Math.min(120, left)), () => after(left))
      : after(left);
    Promise.race([ready, after(left), stood]).then(() => {
      requestAnimationFrame(() => {
        if (el.isConnected && el.dataset.turn === "wait") this.start(el);
      });
    });
  },
  start(el) {
    el.removeAttribute("data-turn");
    this.since = 0;
    const stage = el.closest(".stage");
    const past = this.past;
    if (stage?.style.height) {
      const from = stage.style.height;
      stage.style.height = "";
      const to = stage.offsetHeight;
      stage.style.height = from;
      void stage.offsetHeight;
      stage.dataset.turning = "";
      stage.style.height = `${to}px`;
    }
    past?.classList.add(el.classList.contains("from-next") ? "to-next" : "to-prev");
    const running = [...(past?.getAnimations() ?? []), ...(stage?.getAnimations() ?? [])];
    Promise.allSettled(running.map((one) => one.finished)).then(() => {
      if (this.past === past && !this.since) this.end();
    });
  },
  end() {
    clearTimeout(this.timer);
    document.querySelector('.stage [data-turn="wait"]')?.removeAttribute("data-turn");
    this.past?.remove();
    this.past = null;
    this.since = 0;
    const stage = document.querySelector(".stage");
    if (stage) {
      delete stage.dataset.turning;
      stage.style.height = "";
    }
  },
};
"##;

/// Before going to the previous or next post. Returns once the copy of the
/// picture can stand in for it.
pub async fn hold() {
    let _ = document::eval(&format!(
        "{TURN}\nawait window.__sbTurn.hold(); return true"
    ))
    .join::<bool>()
    .await;
}

/// The new post's picture is on the page, out of sight.
pub fn turn(thumb: Option<&str>) {
    let thumb = serde_json::to_string(&thumb).unwrap_or_else(|_| "null".into());
    document::eval(&format!("{TURN}\nwindow.__sbTurn.show({thumb});"));
}

/// Loads and decodes pictures that may show next, and keeps hold of them: an
/// image no script holds on to is let go, decoded or not.
pub fn ahead(urls: &[String]) {
    let urls = serde_json::to_string(urls).unwrap_or_else(|_| "[]".into());
    document::eval(&format!(
        r#"window.__sbAhead ??= new Map();
for (const url of {urls}) {{
  if (window.__sbAhead.has(url)) continue;
  const img = new Image();
  img.src = url;
  img.decode().catch(() => {{}});
  window.__sbAhead.set(url, img);
}}
for (const url of [...window.__sbAhead.keys()].slice(0, -{AHEAD_KEPT})) window.__sbAhead.delete(url);"#
    ));
}

/// Two neighbours with a picture and a thumbnail each, and the two before.
const AHEAD_KEPT: usize = 8;

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
    fn the_rail_at_the_side_leaves_the_whole_height() {
        // A floating rail starts partway down the screen, which isn't a floor.
        let (_, body) = HERO.split_once("home(id, top, height) {").unwrap();
        assert!(body.contains("dock?.width > dock?.height"));
    }

    #[test]
    fn opening_notes_where_the_gallery_is() {
        // Regression: a click within 200 ms of scrolling came back to an older spot.
        let (_, body) = HERO.split_once("open(id) {").unwrap();
        assert!(body.contains("__sbSpot?.report()"));
    }

    #[test]
    fn a_picture_waiting_for_its_turn_has_not_started_moving() {
        // Regression: the entrance began before the picture could paint, so it
        // showed up partly faded, after a moment of empty stage.
        let css = include_str!("../assets/main.css");
        let (_, rule) = css
            .split_once(r#".stage video[data-turn="wait"] {"#)
            .unwrap();
        let (rule, _) = rule.split_once('}').unwrap();
        assert!(rule.contains("animation: none"));
        assert!(rule.contains("opacity: 0"));
    }

    #[test]
    fn a_turn_always_ends() {
        // Whatever goes wrong, no picture stays out of sight.
        let (_, hold) = TURN.split_once("async hold() {").unwrap();
        assert!(hold.contains("setTimeout(() => this.end()"));
        let (_, end) = TURN.split_once("  end() {").unwrap();
        assert!(end.contains(r#"removeAttribute("data-turn")"#));
    }

    #[test]
    fn the_old_picture_moves_when_the_new_one_does() {
        let (_, start) = TURN.split_once("  start(el) {").unwrap();
        let (start, _) = start.split_once("\n  },").unwrap();
        let shown = start.find(r#"removeAttribute("data-turn")"#).unwrap();
        let moved = start.find("classList.add(").unwrap();
        assert!(shown < moved);
        assert!(!TURN.contains("startViewTransition"));
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
