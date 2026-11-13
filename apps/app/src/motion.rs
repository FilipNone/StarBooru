//! The picture that grows from its tile into the viewer, and shrinks back when
//! leaving. A copy of it floats above the page while the page underneath
//! changes. This is plain script rather than a view transition, as WebKitGTK
//! (the Linux desktop webview) aborts inside `startViewTransition`.

use dioxus::prelude::*;

/// The copy flies over the page, under the viewer's controls and panels and under
/// the bars, as `.hero-past` lies under it. Its flight's values are spelled out, as
/// the script has no use for tokens.
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
    document.documentElement.classList.remove("sb-hero", "sb-hero-back", "sb-hero-flying");
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
      position: "fixed", zIndex: "1", margin: "0", pointerEvents: "none", objectFit: "cover",
      left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px`,
      borderRadius: radius,
    });
    if (under) {
      ghost.style.background = `center / cover no-repeat url(${JSON.stringify(under)})`;
    }
    (document.querySelector(".app") ?? document.body).append(ghost);
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
    // The grid keeps tiles for a few screens on the page, and only those on the
    // screen can be seen in the copy. The rest would be styled, laid out and
    // painted for nothing, while the viewer waits to open.
    const tiles = page.querySelectorAll(".masonry .thumb");
    const copies = copy.querySelectorAll(".masonry .thumb");
    tiles.forEach((each, i) => {
      const at = each.getBoundingClientRect();
      if (at.bottom <= 0 || at.top >= innerHeight) copies[i]?.remove();
    });
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
  // The viewer's controls and panels now stand over the gallery's copy, so they
  // come in as it goes.
  fadeInViewer() {
    for (const el of document.querySelectorAll(".stage > button, .stage-tools, .post-main > :not(.stage), .post-layout > .side")) {
      el.animate([{ opacity: 0 }, {}], { duration: 200, easing: "ease-out" });
    }
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
    const box = tile.getBoundingClientRect();
    // History can open a post whose tile has scrolled out of sight.
    if (box.bottom <= 0 || box.top >= innerHeight) return;
    this.float(img.currentSrc || img.src, box, getComputedStyle(tile).borderRadius);
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
        document.documentElement.classList.add("sb-hero-flying");
        this.fadePast();
        this.fadeInViewer();
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

/// The browser's own forward and back open a post without a click on its tile.
/// By `popstate` the router has already taken the gallery away, but while the
/// gallery is being dropped it's all still on the page.
const BY_HISTORY: &str = r##"
const id = location.pathname.match(/\/post\/(\d+)\/?$/)?.[1];
if (id && !window.__sbHero.ghost && document.querySelector(`.masonry [data-post="${id}"]`)) {
  window.__sbHero.open(Number(id));
}
"##;

/// The gallery is going. Whatever took it to a post without a click on the tile,
/// the picture lifts off just the same. Call before the gallery's other farewells.
pub fn leaving_gallery() {
    document::eval(&format!("{HERO}\n{BY_HISTORY}"));
}

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
    // Where a swipe let the picture go, so the copy carries on from there.
    const at = window.__sbStage?.take() ?? null;
    // Nothing new has shown yet: what's held stays.
    if (this.since) return;
    const stage = document.querySelector(".stage");
    if (!stage) return;
    this.past?.remove();
    this.past = null;
    this.since = performance.now();
    const el = document.getElementById("stage-img");
    if (!el) return;
    if (at) el.dataset.gone = "";
    const copy = el.cloneNode(false);
    copy.removeAttribute("data-gone");
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
    if (at) {
      copy.style.translate = `${at.x}px 0`;
      copy.style.setProperty("--past-x", `${at.x}px`);
    }
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
    const quick = el.classList.contains("quick");
    const ready = quick
      // Flipping through: only the thumbnail is coming for now.
      ? after(left)
      : el.tagName === "VIDEO"
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
    if (el.classList.contains("quick")) {
      this.end();
      return;
    }
    el.removeAttribute("data-turn");
    this.since = 0;
    const past = this.past;
    past?.classList.add(el.classList.contains("from-next") ? "to-next" : "to-prev");
    Promise.allSettled((past?.getAnimations() ?? []).map((one) => one.finished)).then(() => {
      if (this.past === past && !this.since) this.end();
    });
  },
  end() {
    clearTimeout(this.timer);
    document.querySelector('.stage [data-turn="wait"]')?.removeAttribute("data-turn");
    this.past?.remove();
    this.past = null;
    this.since = 0;
    for (const gone of document.querySelectorAll(".stage [data-gone]")) gone.removeAttribute("data-gone");
  },
};
"##;

/// The light behind the viewer. The picture is smudged once onto a tiny canvas,
/// which costs one drawing where a CSS blur would cost every frame of the fade,
/// and only opacity moves after that. One fade runs at a time and then goes on to
/// the latest picture, so flipping through costs no more than a slow drift of
/// colour, and a picture only starts to fade in once it can paint.
const GLOW: &str = r##"
window.__sbGlow ??= {
  wanted: null, busy: false,
  want(url) {
    this.wanted = url;
    if (!this.busy && !document.querySelector('.app[data-saver="on"], .app[data-effects="lite"]')) this.run();
  },
  paint(el, img) {
    const box = el.getBoundingClientRect();
    const aspect = box.width > 0 && box.height > 0 ? box.width / box.height : 1;
    const size = (long) => aspect >= 1
      ? [long, Math.max(2, Math.round(long / aspect))]
      : [Math.max(2, Math.round(long * aspect)), long];
    const scene = (long) => {
      const canvas = document.createElement("canvas");
      [canvas.width, canvas.height] = size(long);
      return canvas;
    };
    const [w, h] = [img.naturalWidth, img.naturalHeight];
    const tiny = scene(24);
    const copy = scene(24);
    const middle = scene(64);
    const crop = Math.min(w / middle.width, h / middle.height);
    const [cw, ch] = [middle.width * crop, middle.height * crop];
    const down = middle.getContext("2d");
    const small = tiny.getContext("2d");
    const held = copy.getContext("2d");
    const out = el.getContext("2d");
    if (!down || !small || !held || !out) return false;
    down.imageSmoothingQuality = small.imageSmoothingQuality = out.imageSmoothingQuality = "high";
    down.drawImage(img, (w - cw) / 2, (h - ch) / 2, cw, ch, 0, 0, middle.width, middle.height);
    small.drawImage(middle, 0, 0, tiny.width, tiny.height);
    // More colour, where the picture has any: the grey between two colours would
    // turn into a third if this came after they're mixed. It can't add any to
    // what's transparent.
    small.globalCompositeOperation = "saturation";
    small.globalAlpha = 0.4;
    small.fillStyle = "#f00";
    small.fillRect(0, 0, tiny.width, tiny.height);
    small.globalCompositeOperation = "destination-in";
    small.globalAlpha = 1;
    small.drawImage(middle, 0, 0, tiny.width, tiny.height);
    small.globalCompositeOperation = "source-over";
    // Averaged with copies of itself all around, or scaling up shows its grid.
    for (const r of [3, 2, 1]) {
      held.clearRect(0, 0, copy.width, copy.height);
      held.drawImage(tiny, 0, 0);
      let seen = 1;
      for (const [x, y] of [[r, 0], [-r, 0], [0, r], [0, -r], [r, r], [-r, -r], [r, -r], [-r, r]]) {
        small.globalAlpha = 1 / ++seen;
        small.drawImage(copy, x, y);
      }
    }
    small.globalAlpha = 1;
    [el.width, el.height] = size(96);
    out.drawImage(tiny, 0, 0, el.width, el.height);
    return true;
  },
  async run() {
    this.busy = true;
    const after = (ms) => new Promise((done) => setTimeout(done, ms));
    while (true) {
      const url = this.wanted;
      const root = document.querySelector(".post-glow");
      if (!url || !root || root.querySelector(".on")?.dataset.url === url) break;
      const img = new Image();
      img.src = url;
      const ok = await Promise.race([img.decode().then(() => true, () => false), after(3000).then(() => false)]);
      if (!ok) {
        if (url === this.wanted) break;
        continue;
      }
      if (!root.isConnected) continue;
      const front = root.querySelector(".on");
      const back = [...root.querySelectorAll(".glow-layer")].find((layer) => layer !== front);
      if (!back || !this.paint(back, img)) break;
      back.dataset.url = url;
      back.classList.add("on");
      front?.classList.remove("on");
      await Promise.race([Promise.allSettled(back.getAnimations().map((one) => one.finished)), after(1000)]);
    }
    this.busy = false;
  },
};
"##;

/// The picture the light behind the viewer should come from.
pub fn glow(url: Option<&str>) {
    let url = serde_json::to_string(&url).unwrap_or_else(|_| "null".into());
    document::eval(&format!("{GLOW}\nwindow.__sbGlow.want({url});"));
}

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

/// Pictures that may show next, loaded and decoded ahead and kept hold of: an
/// image no script holds on to is let go, decoded or not. They download
/// together and decode one after another, as decoding two full pictures and
/// their thumbnails at once takes the processor from what's on the screen.
const AHEAD: &str = r##"
window.__sbAhead ??= {
  kept: new Map(), waiting: [], busy: false,
  want(urls, keep) {
    for (const url of urls) {
      if (this.kept.has(url)) continue;
      const img = new Image();
      img.src = url;
      this.kept.set(url, img);
      this.waiting.push(img);
    }
    for (const url of [...this.kept.keys()].slice(0, -keep)) this.kept.delete(url);
    if (!this.busy) this.run();
  },
  async run() {
    this.busy = true;
    while (this.waiting.length) await this.waiting.shift().decode().catch(() => {});
    this.busy = false;
  },
};
"##;

/// Gets pictures that may show next ready to paint. See [`AHEAD`].
pub fn ahead(urls: &[String]) {
    let urls = serde_json::to_string(urls).unwrap_or_else(|_| "[]".into());
    document::eval(&format!(
        "{AHEAD}\nwindow.__sbAhead.want({urls}, {AHEAD_KEPT});"
    ));
}

/// Two neighbours with a picture and a thumbnail each, and the two before.
const AHEAD_KEPT: usize = 8;

/// Resolves once a post's opening is over: the picture has flown in or turned,
/// its file has come, the light behind it has changed and the page has stopped
/// scrolling. It looks every 60 ms and wants two calm looks in a row. A file
/// that takes longer than three seconds isn't waited for.
const REST: &str = r##"
// A spinner turns for ever, and is nothing to wait for.
const moving = (node) => (node?.getAnimations({ subtree: true }) ?? [])
  .some((one) => one.playState === "running" && Number.isFinite(one.effect?.getComputedTiming().endTime));
const busy = () => {
  const shown = document.querySelector("#stage-img, .stage video");
  return !!window.__sbHero?.ghost || !!window.__sbHero?.past || !!window.__sbTurn?.since
    || (shown?.tagName === "IMG" && !shown.complete)
    || moving(document.querySelector(".stage")) || moving(document.querySelector(".post-glow"));
};
const until = performance.now() + 3000;
let calm = 0, y = scrollY;
while (calm < 2 && performance.now() < until) {
  await new Promise((done) => setTimeout(done, 60));
  calm = !busy() && scrollY === y ? calm + 1 : 0;
  y = scrollY;
}
return true;
"##;

/// Waits for the viewer to come to rest after a post opens. What isn't the
/// picture itself (the related posts, the neighbours fetched ahead) waits for
/// this, so it doesn't load and decode while the picture is still arriving.
pub async fn rested() {
    let _ = document::eval(REST).join::<bool>().await;
}

/// Resolves when the picture `selector` finds can paint, has failed, or has
/// taken more than a second and a half, and a frame has passed since. A slow
/// one goes on loading: it only stops holding up whoever waits here.
pub async fn decoded(selector: &str) {
    let selector = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
    let _ = document::eval(&format!(
        r#"const img = document.querySelector({selector});
await Promise.race([img?.decode().catch(() => {{}}), new Promise((done) => setTimeout(done, 1500))]);
await new Promise(requestAnimationFrame);
return true;"#
    ))
    .join::<bool>()
    .await;
}

/// Back to the top of the gallery without scrolling past everything in between.
/// Copies of the tiles on the screen float over the page, the page jumps to the
/// top under them, and each copy moves up its column to the place of a tile at
/// the top while that tile's picture fades in over its own. The grid itself
/// stays hidden until they have landed, and where there are more tiles on one
/// side than the other, the extra ones fade. Any wheel, touch, click or key
/// stops it and shows the grid as it is.
const TO_TOP: &str = r##"
window.__sbTop ??= {
  layer: null, timer: 0, head: [],
  calm() {
    return matchMedia("(prefers-reduced-motion: reduce)").matches
      || !!document.querySelector('.app[data-saver="on"]');
  },
  cut() {
    window.__sbTop.clear();
  },
  clear() {
    clearTimeout(this.timer);
    this.layer?.remove();
    this.layer = null;
    for (const el of this.head) el.style.visibility = "";
    this.head = [];
    for (const kind of ["wheel", "touchstart", "pointerdown", "keydown"]) removeEventListener(kind, this.cut);
    document.documentElement.classList.remove("sb-to-top");
  },
  // The tiles on the screen, in the grid's order.
  seen() {
    const out = [];
    for (const tile of document.querySelectorAll(".masonry .thumb")) {
      const box = tile.getBoundingClientRect();
      if (box.bottom <= 0 || box.top >= innerHeight || !box.width) continue;
      out.push({ box, img: tile.querySelector(".thumb-media > img"), tint: tile.style.getPropertyValue("--tint") });
    }
    return out;
  },
  // The tiles on the screen once the app has put the top of the grid there.
  top() {
    const grid = document.querySelector(".masonry")?.getBoundingClientRect();
    const tiles = this.seen();
    return grid && tiles.some((t) => t.box.top - grid.top < 1) ? tiles : [];
  },
  // A tile's colour, and its picture if it has one to show yet.
  copy(item, name) {
    const el = document.createElement("div");
    el.className = name;
    if (item.tint) el.style.setProperty("--tint", item.tint);
    const img = item.img;
    if (img?.complete && img.naturalWidth) {
      const pic = new Image();
      pic.alt = "";
      pic.decoding = "sync";
      pic.src = img.currentSrc || img.src;
      el.append(pic);
    }
    return el;
  },
  async go() {
    this.clear();
    const jump = () => {
      scrollTo({ top: 0, behavior: "instant" });
      window.__sbNear?.check(true);
    };
    const app = document.querySelector(".app");
    const from = this.seen();
    if (!app || !from.length || this.calm()) return jump();
    const at = (b) => ({ left: `${b.left}px`, top: `${b.top}px`, width: `${b.width}px`, height: `${b.height}px` });
    const layer = document.createElement("div");
    layer.className = "to-top-layer";
    layer.setAttribute("aria-hidden", "true");
    const olds = from.map((item) => {
      const el = this.copy(item, "to-top-tile");
      Object.assign(el.style, at(item.box));
      layer.append(el);
      return el;
    });
    app.append(layer);
    this.layer = layer;
    document.documentElement.classList.add("sb-to-top");
    this.timer = setTimeout(() => this.clear(), 3000);
    for (const kind of ["wheel", "touchstart", "pointerdown", "keydown"]) {
      addEventListener(kind, this.cut, { passive: true });
    }
    jump();
    // The top of the grid comes once the app has heard where the page is.
    let to = [];
    for (let frame = 0; frame < 30 && !to.length; frame++) {
      await new Promise(requestAnimationFrame);
      if (this.layer !== layer) return;
      to = this.top();
    }
    if (!to.length) to = this.seen();
    // Their pictures decoded, or their colours if that takes too long.
    await Promise.race([
      Promise.allSettled(to.map((t) => t.img?.decode?.())),
      new Promise((done) => setTimeout(done, 300)),
    ]);
    if (this.layer !== layer) return;
    to = this.seen();
    const ease = "cubic-bezier(0.2, 0, 0, 1)";
    const time = 560;
    // A wave from the top left corner.
    const wave = (b) => Math.round(Math.max(0, b.top) / innerHeight * 140 + b.left / innerWidth * 60);
    // Each tile goes to the tile at the top in the same column and the same
    // place down it, so it moves up its column and doesn't cross the others.
    const columns = (list) => {
      const by = new Map();
      for (const item of list) {
        const key = Math.round(item.box.left);
        if (!by.has(key)) by.set(key, []);
        by.get(key).push(item);
      }
      for (const column of by.values()) column.sort((a, b) => a.box.top - b.box.top);
      return by;
    };
    const tops = columns(to);
    const taken = new Set();
    const moves = [];
    for (const [key, column] of columns(from.map((item, i) => ({ ...item, el: olds[i] })))) {
      column.forEach((item, rank) => {
        const goal = tops.get(key)?.[rank];
        const el = item.el;
        if (!goal) {
          moves.push(el.animate([{ opacity: 1 }, { opacity: 0, transform: "scale(0.9)" }],
            { duration: time / 2, delay: wave(item.box), easing: ease, fill: "both" }));
          return;
        }
        taken.add(goal);
        const next = this.copy(goal, "to-top-next");
        el.append(next);
        moves.push(el.animate([at(item.box), at(goal.box)],
          { duration: time, delay: wave(goal.box), easing: ease, fill: "both" }));
        next.animate([{ opacity: 0 }, { opacity: 1 }],
          { duration: time * 0.7, delay: wave(goal.box) + time * 0.15, easing: "ease-in-out", fill: "both" });
      });
    }
    for (const goal of to.filter((t) => !taken.has(t))) {
      const el = this.copy(goal, "to-top-tile");
      Object.assign(el.style, at(goal.box));
      layer.append(el);
      moves.push(el.animate([{ opacity: 0, transform: "scale(0.9)" }, { opacity: 1, transform: "none" }],
        { duration: time, delay: wave(goal.box) + time / 3, easing: ease, fill: "both" }));
    }
    // The title and the menus above the grid come down into their places.
    this.head = [...document.querySelectorAll(".page.gallery > :not(.masonry-box)")];
    for (const el of this.head) {
      el.style.visibility = "visible";
      el.animate([{ opacity: 0, transform: "translateY(-12px)" }, { opacity: 1, transform: "none" }],
        { duration: time, delay: time / 3, easing: ease, fill: "backwards" });
    }
    await Promise.allSettled(moves.map((m) => m.finished));
    if (this.layer !== layer) return;
    // The copies end where the tiles are, so the grid shows under them a
    // frame before they go.
    document.documentElement.classList.remove("sb-to-top");
    await new Promise(requestAnimationFrame);
    if (this.layer === layer) this.clear();
  },
};
"##;

/// Takes the gallery back to its top, its tiles moving to their new places.
pub fn to_top() {
    document::eval(&format!("{TO_TOP}\nwindow.__sbTop.go();"));
}

/// The gallery is going: its tiles stop wherever they were.
pub fn stop_to_top() {
    document::eval(&format!("{TO_TOP}\nwindow.__sbTop.clear();"));
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
    fn the_flying_picture_stays_under_the_bars() {
        // Regression: it hung off `<body>` above everything, so it crossed over the
        // bars and the rail. Lower numbers come first: the gallery's copy, the
        // picture, then the bars.
        let css = include_str!("../assets/main.css");
        assert!(css.contains(".hero-past { position: fixed; inset: 0; z-index: 0;"));
        assert!(HERO.contains(r#"zIndex: "1""#));
        assert!(HERO.contains(r#"document.querySelector(".app")"#));
        assert!(
            css.contains("position: fixed; z-index: 9; left: var(--bar-gap); width: var(--rail-w)")
        );
        assert!(css.contains("position: fixed; inset: 0 0 auto; z-index: 10;"));
    }

    #[test]
    fn tiles_going_to_the_top_stay_under_the_bars() {
        let css = include_str!("../assets/main.css");
        assert!(css.contains(".to-top-layer { position: fixed; inset: 0; z-index: 1;"));
        assert!(TO_TOP.contains(r#"const app = document.querySelector(".app");"#));
        assert!(TO_TOP.contains("app.append(layer);"));
    }

    #[test]
    fn tiles_going_to_the_top_keep_to_their_columns() {
        // Paired in the grid's order, a tile at the bottom of one column flew
        // across the screen to the top of another, through all the rest.
        assert!(TO_TOP.contains("const key = Math.round(item.box.left);"));
        assert!(TO_TOP.contains("const goal = tops.get(key)?.[rank];"));
    }

    #[test]
    fn going_to_the_top_jumps_while_motion_is_calmed() {
        assert!(TO_TOP.contains(r#"matchMedia("(prefers-reduced-motion: reduce)").matches"#));
        assert!(TO_TOP.contains("if (!app || !from.length || this.calm()) return jump();"));
    }

    #[test]
    fn going_to_the_top_always_ends() {
        // A wheel, touch, click or key stops it, and if nothing does and the
        // tiles never come, a timer does. Each way shows the grid again.
        assert!(TO_TOP.contains("this.timer = setTimeout(() => this.clear(), 3000);"));
        let (_, clear) = TO_TOP.split_once("clear() {").unwrap();
        let (clear, _) = clear.split_once("\n  },").unwrap();
        assert!(clear.contains(r#"classList.remove("sb-to-top")"#));
        assert!(clear.contains("removeEventListener(kind, this.cut)"));
        assert!(clear.contains("this.layer?.remove();"));
        assert!(include_str!("pages/gallery.rs").contains("motion::stop_to_top();"));
    }

    #[test]
    fn the_grid_shows_again_before_the_copies_go() {
        // The copies end where the tiles are; taking them away first would
        // leave the page empty for a frame.
        let shown = TO_TOP.rfind(r#"classList.remove("sb-to-top")"#).unwrap();
        let gone = TO_TOP
            .rfind("if (this.layer === layer) this.clear();")
            .unwrap();
        assert!(shown < gone);
        let css = include_str!("../assets/main.css");
        assert!(css.contains("html.sb-to-top .page.gallery { visibility: hidden; }"));
    }

    #[test]
    fn the_flying_picture_stays_under_the_viewers_controls_and_panels() {
        // Regression: it covered back, previous/next, the zoom tools, the bar and
        // the panels until it landed. The buttons are at 2 inside the stage, which
        // lets them out only while the picture flies.
        let css = include_str!("../assets/main.css");
        assert!(css.contains(".stage-tools, .stage-translate, .stage-note { z-index: 2; }"));
        assert!(css.contains("html.sb-hero-flying .stage { isolation: auto; }"));
        assert!(css.contains(
            "html.sb-hero-flying :is(.post-bar, .post-main > .banner, .related, .post-layout > .side) { position: relative; z-index: 2; }"
        ));
        // Every child of `.post-main` is the stage or one of those: one that
        // isn't would be flown over. And none of them is "not the stage", which
        // costs a restyle of the whole page each time the class comes and goes.
        assert!(!css.contains("html.sb-hero-flying :is(.post-main > :not("));
        let viewer = include_str!("pages/post.rs");
        let (_, main) = viewer.split_once(r#"div { class: "post-main","#).unwrap();
        let (main, _) = main.split_once(r#"aside { class: "side","#).unwrap();
        for child in [
            "Stage {",
            r#"class: "post-bar glass float""#,
            "ErrorBanner {",
            r#"class: "banner warn""#,
            "Related {",
        ] {
            assert!(main.contains(child), "{child}");
        }
        assert!(
            include_str!("pages/mod.rs").contains(r#"div { class: "banner error", role: "alert","#)
        );
        assert!(viewer.contains(r#"section { class: "related","#));
        // Set before the flight and gone with the picture, so no button changes
        // places with it while it's still there.
        let (_, go) = HERO.split_once("const go = (tries) => {").unwrap();
        let set = go.find(r#"classList.add("sb-hero-flying")"#).unwrap();
        assert!(set < go.find("this.fly(").unwrap());
        let (_, clear) = HERO.split_once("clear() {").unwrap();
        let (clear, _) = clear.split_once("\n  },").unwrap();
        assert!(clear.contains("this.ghost?.remove()") && clear.contains(r#""sb-hero-flying""#));
    }

    #[test]
    fn a_sliding_picture_stays_under_the_panels() {
        // Regression: the panels weren't positioned, so a picture sliding in from
        // the side was painted over them. With `order`, it still would be.
        let css = include_str!("../assets/main.css");
        assert!(css.contains(".post-layout > .side { position: relative; }"));
        assert!(!css.contains("order: -1"));
    }

    #[test]
    fn the_browsers_forward_button_flies_like_a_click() {
        // Regression: only a click on a tile lifted the picture off, so forward
        // and back to a post skipped the opening.
        assert!(BY_HISTORY.contains(".masonry [data-post="));
        // A click has lifted it off already.
        assert!(BY_HISTORY.contains("!window.__sbHero.ghost"));
        // While the gallery is on the page: a `popstate` listener comes too late.
        assert!(!BY_HISTORY.contains("popstate"));
        let gallery = include_str!("pages/gallery.rs");
        let (_, drop) = gallery.split_once("use_drop(|| {").unwrap();
        let farewell = drop.find("motion::leaving_gallery()").unwrap();
        assert!(farewell < drop.find("WATCH_SCROLL_OFF").unwrap());
        assert!(HERO.contains("box.bottom <= 0 || box.top >= innerHeight"));
    }

    #[test]
    fn the_glow_waits_for_its_picture_and_takes_one_fade_at_a_time() {
        let (_, run) = GLOW.split_once("async run() {").unwrap();
        let decode = run.find("img.decode()").unwrap();
        let paint = run.find("this.paint(").unwrap();
        let fade = run.find(r#"back.classList.add("on")"#).unwrap();
        assert!(decode < paint && paint < fade);
        assert!(run.contains("getAnimations()"));
        assert!(GLOW.contains("if (!this.busy &&"));
        // It never ends up waiting on a picture that doesn't come.
        assert!(run.contains("after(3000)"));
    }

    #[test]
    fn the_glow_needs_nothing_the_webviews_lack() {
        // WebKit has no `ctx.filter`, and a picture from another origin can be drawn
        // but not read back.
        assert!(!GLOW.contains("ctx.filter") && !GLOW.contains(".filter ="));
        assert!(!GLOW.contains("getImageData") && !GLOW.contains("toDataURL"));
        assert!(GLOW.contains(r#"data-saver="on""#));
        // Reduced effects hide the glow, so it isn't drawn either.
        assert!(GLOW.contains(r#".app[data-effects="lite"]"#));
    }

    #[test]
    fn a_picture_flipped_past_has_no_entrance_to_wait_for() {
        let (_, show) = TURN.split_once("show(thumb) {").unwrap();
        assert!(show.contains(r#"el.classList.contains("quick")"#));
        let (_, start) = TURN.split_once("start(el) {").unwrap();
        let (start, _) = start.split_once("\n  },").unwrap();
        assert!(
            start.contains(r#"el.classList.contains("quick")"#) && start.contains("this.end()")
        );
        let css = include_str!("../assets/main.css");
        assert!(css.contains(".stage img.quick, .stage video.quick { animation: none; }"));
    }

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
    fn lifting_off_restyles_no_picture_in_the_gallery() {
        // The viewer's picture stays hidden until the flying one has landed.
        let css = include_str!("../assets/main.css");
        assert!(css.contains("html.sb-hero :is(#stage-img, .stage video) { visibility: hidden; }"));
        assert!(!css.contains("html.sb-hero .stage img"));
        assert!(include_str!("pages/post.rs").contains(r#"id: "stage-img","#));
    }

    #[test]
    fn the_gallery_left_behind_is_only_what_was_on_the_screen() {
        let (_, linger) = HERO.split_once("linger(id, tile) {").unwrap();
        let (linger, _) = linger.split_once("\n  },").unwrap();
        assert!(
            linger.contains("if (at.bottom <= 0 || at.top >= innerHeight) copies[i]?.remove();")
        );
        // Before the copy goes onto the page, so the tiles left out cost nothing.
        assert!(
            linger.find("copies[i]?.remove()").unwrap() < linger.find("app.append(past)").unwrap()
        );
    }

    #[test]
    fn the_wait_for_rest_always_ends() {
        // A picture that never comes, or a page that never stops scrolling,
        // doesn't keep the related posts away.
        assert!(REST.contains("const until = performance.now() + 3000;"));
        assert!(REST.contains("while (calm < 2 && performance.now() < until)"));
        // What it waits for: the flight, the turn, the file, the page.
        for busy in [
            "__sbHero?.ghost",
            "__sbTurn?.since",
            "!shown.complete",
            "scrollY === y",
        ] {
            assert!(REST.contains(busy), "{busy}");
        }
        // A spinner would hold it for the whole three seconds.
        assert!(REST.contains("Number.isFinite(one.effect?.getComputedTiming().endTime)"));
    }

    #[test]
    fn pictures_fetched_ahead_decode_one_at_a_time() {
        assert!(AHEAD.contains(
            "while (this.waiting.length) await this.waiting.shift().decode().catch(() => {});"
        ));
        // All asked for at once, so they download together.
        let (_, want) = AHEAD.split_once("want(urls, keep) {").unwrap();
        let (want, _) = want.split_once("\n  },").unwrap();
        assert!(want.contains("img.src = url;"));
        assert!(!want.contains("await"));
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
