//! The media editor. It fills the window: the picture takes most of it, and
//! the tools stand beside it when the window is wider than tall (on the hand
//! set in Settings) or under it when taller, like the navigation. Images are
//! cropped, straightened, turned and flipped, with undo and redo; videos are
//! trimmed. The server saves the result as a new post, and the original goes
//! to the recycle bin unless the user keeps it (remembered in `Prefs`).

use dioxus::html::geometry::ClientPoint;
use dioxus::prelude::*;
use starbooru_core::{Crop, EditError, EditKind, MAX_STRAIGHTEN, MediaEdit, Post, Trim};

use super::ErrorBanner;
use crate::icons::{Ico, Icon};
use crate::store::{self, PREFS};
use crate::util::format_duration;

/// Smallest crop side, as a share of the picture's shorter side.
const MIN_CROP: f64 = 0.04;
/// How many steps undo can go back.
const MAX_HISTORY: usize = 100;
/// Room around the picture for the crop handles, in pixels.
const FRAME_PAD: f64 = 20.0;
/// Fixed proportions offered, as width by height of the upright version.
const RATIOS: [(u16, u16); 5] = [(1, 1), (3, 4), (2, 3), (4, 5), (9, 16)];
/// The largest side a custom proportion takes.
const MAX_RATIO_SIDE: u16 = 100;

/// `v` limited to `lo..=hi`, where `f64::clamp` would panic on `lo > hi`
/// or NaN; `lo` wins then, and NaN is `lo`.
fn within(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_nan() { lo } else { v.min(hi).max(lo) }
}

/// A crop as shares (0 to 1) of the turned and flipped picture, so it doesn't
/// depend on how big the preview is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CropBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// What a drag holds: the whole box, one side, or a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    Move,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Handle {
    const ALL: [(Handle, &'static str); 8] = [
        (Handle::Top, "n"),
        (Handle::Bottom, "s"),
        (Handle::Left, "w"),
        (Handle::Right, "e"),
        (Handle::TopLeft, "nw"),
        (Handle::TopRight, "ne"),
        (Handle::BottomLeft, "sw"),
        (Handle::BottomRight, "se"),
    ];

    /// Which sides move: -1 the left (or top), 1 the right (or bottom), 0 neither.
    fn sides(self) -> (i8, i8) {
        match self {
            Handle::Move => (0, 0),
            Handle::Top => (0, -1),
            Handle::Bottom => (0, 1),
            Handle::Left => (-1, 0),
            Handle::Right => (1, 0),
            Handle::TopLeft => (-1, -1),
            Handle::TopRight => (1, -1),
            Handle::BottomLeft => (-1, 1),
            Handle::BottomRight => (1, 1),
        }
    }
}

/// The edges of a box in pixels.
#[derive(Debug, Clone, Copy)]
struct Edges {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
}

impl CropBox {
    const FULL: CropBox = CropBox {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    fn edges(self, fw: f64, fh: f64) -> Edges {
        Edges {
            l: self.x * fw,
            t: self.y * fh,
            r: (self.x + self.w) * fw,
            b: (self.y + self.h) * fh,
        }
    }

    /// Back from pixels to shares. Anything odd (a zero-sized frame, say)
    /// gives the whole picture.
    fn from_edges(e: Edges, fw: f64, fh: f64) -> CropBox {
        let b = CropBox {
            x: e.l / fw,
            y: e.t / fh,
            w: (e.r - e.l) / fw,
            h: (e.b - e.t) / fh,
        };
        if [b.x, b.y, b.w, b.h].iter().all(|v| v.is_finite()) && b.w > 0.0 && b.h > 0.0 {
            b
        } else {
            CropBox::FULL
        }
    }

    /// The same part of the picture after it is mirrored.
    fn mirrored(self, horizontal: bool) -> CropBox {
        if horizontal {
            CropBox {
                x: 1.0 - self.x - self.w,
                ..self
            }
        } else {
            CropBox {
                y: 1.0 - self.y - self.h,
                ..self
            }
        }
    }

    /// The same part of the picture after it turns a quarter.
    fn turned(self, clockwise: bool) -> CropBox {
        if clockwise {
            CropBox {
                x: 1.0 - self.y - self.h,
                y: self.x,
                w: self.h,
                h: self.w,
            }
        } else {
            CropBox {
                x: self.y,
                y: 1.0 - self.x - self.w,
                w: self.h,
                h: self.w,
            }
        }
    }

    /// The biggest box of `ratio` (width over height, in pixels) that fits
    /// in a `fw` × `fh` frame, as near the middle of this one as it can be.
    fn with_ratio(self, ratio: f64, fw: f64, fh: f64) -> CropBox {
        if !(ratio.is_finite() && ratio > 0.0 && fw > 0.0 && fh > 0.0) {
            return self;
        }
        let w = fw.min(fh * ratio);
        let h = w / ratio;
        let e = self.edges(fw, fh);
        let (cx, cy) = ((e.l + e.r) / 2.0, (e.t + e.b) / 2.0);
        let l = within(cx - w / 2.0, 0.0, fw - w);
        let t = within(cy - h / 2.0, 0.0, fh - h);
        CropBox::from_edges(
            Edges {
                l,
                t,
                r: l + w,
                b: t + h,
            },
            fw,
            fh,
        )
    }

    /// Moves `handle` by `dx`, `dy` pixels of a `fw` × `fh` frame, keeping
    /// the box inside it, no smaller than `MIN_CROP`, and at `ratio` (width
    /// over height in pixels) when there is one.
    fn dragged(
        self,
        handle: Handle,
        dx: f64,
        dy: f64,
        fw: f64,
        fh: f64,
        ratio: Option<f64>,
    ) -> CropBox {
        if !(fw > 0.0 && fh > 0.0) {
            return self;
        }
        let e = self.edges(fw, fh);
        let (w, h) = (e.r - e.l, e.b - e.t);
        let min = MIN_CROP * fw.min(fh);
        let (hx, hy) = handle.sides();
        let out = match (handle, ratio.filter(|r| r.is_finite() && *r > 0.0)) {
            (Handle::Move, _) => {
                let l = within(e.l + dx, 0.0, fw - w);
                let t = within(e.t + dy, 0.0, fh - h);
                Edges {
                    l,
                    t,
                    r: l + w,
                    b: t + h,
                }
            }
            (_, None) => {
                let mut n = e;
                match hx {
                    -1 => n.l = within(e.l + dx, 0.0, e.r - min),
                    1 => n.r = within(e.r + dx, e.l + min, fw),
                    _ => {}
                }
                match hy {
                    -1 => n.t = within(e.t + dy, 0.0, e.b - min),
                    1 => n.b = within(e.b + dy, e.t + min, fh),
                    _ => {}
                }
                n
            }
            (_, Some(ratio)) => {
                // The shorter side stays at least `min`.
                let min_w = min * ratio.max(1.0);
                // Room from the side that stays put, or around the middle
                // for the direction a side handle doesn't move.
                let room_w = match hx {
                    1 => fw - e.l,
                    -1 => e.r,
                    _ => 2.0 * ((e.l + e.r) / 2.0).min(fw - (e.l + e.r) / 2.0),
                };
                let room_h = match hy {
                    1 => fh - e.t,
                    -1 => e.b,
                    _ => 2.0 * ((e.t + e.b) / 2.0).min(fh - (e.t + e.b) / 2.0),
                };
                let wanted_w = w + f64::from(hx) * dx;
                let wanted_h = h + f64::from(hy) * dy;
                // A corner follows whichever way the pointer went further.
                let wanted = match (hx, hy) {
                    (0, _) => wanted_h * ratio,
                    (_, 0) => wanted_w,
                    _ => wanted_w.max(wanted_h * ratio),
                };
                let nw = wanted.max(min_w).min(room_w).min(room_h * ratio);
                let nh = nw / ratio;
                let l = match hx {
                    1 => e.l,
                    -1 => e.r - nw,
                    _ => (e.l + e.r) / 2.0 - nw / 2.0,
                };
                let t = match hy {
                    1 => e.t,
                    -1 => e.b - nh,
                    _ => (e.t + e.b) / 2.0 - nh / 2.0,
                };
                Edges {
                    l,
                    t,
                    r: l + nw,
                    b: t + nh,
                }
            }
        };
        CropBox::from_edges(out, fw, fh)
    }

    /// The crop in pixels of a `width` × `height` picture; `None` when it
    /// keeps the whole picture. Always at least one pixel, inside the picture.
    fn pixels(self, width: u32, height: u32) -> Option<Crop> {
        let side = |at: f64, len: f64, max: u32| {
            let m = f64::from(max);
            let start = within((at * m).round(), 0.0, (m - 1.0).max(0.0));
            let end = within(((at + len) * m).round(), start + 1.0, m.max(start + 1.0));
            // Whole numbers within 0..=max, so the casts are exact.
            (start as u32, (end - start) as u32)
        };
        let (x, w) = side(self.x, self.w, width);
        let (y, h) = side(self.y, self.h, height);
        let crop = Crop {
            x,
            y,
            width: w,
            height: h,
        };
        (crop
            != Crop {
                x: 0,
                y: 0,
                width,
                height,
            })
        .then_some(crop)
    }

    fn style(self) -> String {
        format!(
            "left: {:.3}%; top: {:.3}%; width: {:.3}%; height: {:.3}%",
            self.x * 100.0,
            self.y * 100.0,
            self.w * 100.0,
            self.h * 100.0
        )
    }
}

/// The crop's proportions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aspect {
    Free,
    /// The picture's own, as turned.
    Original,
    /// Width by height.
    Fixed(u16, u16),
}

impl Aspect {
    /// Width over height in pixels, for a `fw` × `fh` picture.
    fn ratio(self, fw: f64, fh: f64) -> Option<f64> {
        match self {
            Aspect::Free => None,
            Aspect::Original => (fh > 0.0).then(|| fw / fh),
            Aspect::Fixed(w, h) => (h > 0 && w > 0).then(|| f64::from(w) / f64::from(h)),
        }
    }

    /// The same proportions on their side.
    fn swapped(self) -> Aspect {
        match self {
            Aspect::Fixed(w, h) => Aspect::Fixed(h, w),
            other => other,
        }
    }

    /// One of the offered proportions, either way up.
    fn is_preset(self) -> bool {
        match self {
            Aspect::Fixed(w, h) => RATIOS
                .iter()
                .any(|&(a, b)| (a, b) == (w, h) || (b, a) == (w, h)),
            _ => true,
        }
    }
}

/// Reads one side of a custom proportion.
fn ratio_side(text: &str) -> Option<u16> {
    text.trim()
        .parse::<u16>()
        .ok()
        .filter(|v| (1..=MAX_RATIO_SIDE).contains(v))
}

/// Everything undo and redo step through.
#[derive(Debug, Clone, PartialEq)]
struct Shape {
    /// Turns, flips and straightening; its crop and trim stay empty.
    edit: MediaEdit,
    crop: CropBox,
    aspect: Aspect,
    /// The offered proportions are shown wider than tall.
    wide: bool,
}

impl Shape {
    fn new(width: u32, height: u32) -> Shape {
        Shape {
            edit: MediaEdit::default(),
            crop: CropBox::FULL,
            aspect: Aspect::Free,
            wide: width > height,
        }
    }

    /// The picture as turned, in pixels.
    fn frame(&self, width: u32, height: u32) -> (f64, f64) {
        let (w, h) = self.edit.turned_size(width, height);
        (f64::from(w.max(1)), f64::from(h.max(1)))
    }

    /// A quarter turn: the crop and its proportions turn with the picture.
    fn turn(&self, clockwise: bool) -> Shape {
        let mut edit = self.edit.clone();
        edit.turn(clockwise);
        Shape {
            edit,
            crop: self.crop.turned(clockwise),
            aspect: self.aspect.swapped(),
            wide: !self.wide,
        }
    }

    /// A mirror of the whole result: the straightening leans the other way.
    fn flip(&self, horizontal: bool) -> Shape {
        let mut edit = self.edit.clone();
        if horizontal {
            edit.flip_h = !edit.flip_h;
        } else {
            edit.flip_v = !edit.flip_v;
        }
        edit.straighten = edit.straighten.saturating_neg();
        Shape {
            edit,
            crop: self.crop.mirrored(horizontal),
            ..self.clone()
        }
    }

    /// New proportions, with the crop made to fit them.
    fn with_aspect(&self, aspect: Aspect, width: u32, height: u32) -> Shape {
        let (fw, fh) = self.frame(width, height);
        let crop = match aspect.ratio(fw, fh) {
            Some(r) => self.crop.with_ratio(r, fw, fh),
            None => self.crop,
        };
        let wide = match aspect {
            Aspect::Fixed(w, h) if w != h => w > h,
            _ => self.wide,
        };
        Shape {
            crop,
            aspect,
            wide,
            ..self.clone()
        }
    }

    /// The other way up for the offered proportions, and the crop's.
    fn swap_orientation(&self, width: u32, height: u32) -> Shape {
        let flipped = Shape {
            wide: !self.wide,
            ..self.clone()
        };
        match self.aspect {
            Aspect::Fixed(w, h) if w != h => {
                flipped.with_aspect(self.aspect.swapped(), width, height)
            }
            _ => flipped,
        }
    }

    /// The request this shape makes.
    fn request(&self, width: u32, height: u32) -> MediaEdit {
        let mut e = self.edit.clone();
        let (w, h) = e.turned_size(width, height);
        e.crop = self.crop.pixels(w, h);
        e
    }
}

/// Undo and redo.
#[derive(Debug, Clone, Default, PartialEq)]
struct History {
    past: Vec<Shape>,
    future: Vec<Shape>,
}

impl History {
    /// `before` is where a change started from.
    fn record(&mut self, before: Shape) {
        self.past.push(before);
        if self.past.len() > MAX_HISTORY {
            self.past.remove(0);
        }
        self.future.clear();
    }

    fn undo(&mut self, now: &Shape) -> Option<Shape> {
        let back = self.past.pop()?;
        self.future.push(now.clone());
        Some(back)
    }

    fn redo(&mut self, now: &Shape) -> Option<Shape> {
        let ahead = self.future.pop()?;
        self.past.push(now.clone());
        Some(ahead)
    }
}

/// How far the shown picture turns to reach `rotate`, from `spin`: the short
/// way: from 270° to 0° it goes a quarter turn forward.
fn spun(spin: i32, rotate: u16) -> i32 {
    let delta = (i32::from(rotate) - spin).rem_euclid(360);
    spin + if delta > 180 { delta - 360 } else { delta }
}

/// The frame's size in pixels: the turned picture made as big as `room`
/// allows, less space for the handles.
fn frame_size(fw: f64, fh: f64, room: (f64, f64)) -> (f64, f64) {
    let (aw, ah) = (
        (room.0 - 2.0 * FRAME_PAD).max(1.0),
        (room.1 - 2.0 * FRAME_PAD).max(1.0),
    );
    let k = (aw / fw).min(ah / fh);
    if k.is_finite() && k > 0.0 {
        (fw * k, fh * k)
    } else {
        (1.0, 1.0)
    }
}

/// The picture's style inside a frame of `size`: unturned, it is as big as
/// the frame turned back, and then turns, flips, grows and straightens.
/// `spin` is the quarter turn as shown (see [`spun`]).
fn picture_style(edit: &MediaEdit, spin: i32, size: (f64, f64), width: u32, height: u32) -> String {
    let (w, h) = if edit.rotate % 180 == 90 {
        (size.1, size.0)
    } else {
        size
    };
    let (tw, th) = edit.turned_size(width, height);
    let scale = edit.straighten_scale(tw, th);
    let sx = if edit.flip_h { -1 } else { 1 };
    let sy = if edit.flip_v { -1 } else { 1 };
    // Applied right to left: turn, flip, grow, straighten, centre.
    format!(
        "width: {w:.2}px; height: {h:.2}px; transform: translate(-50%, -50%) rotate({:.1}deg) scale({scale:.4}) scale({sx}, {sy}) rotate({spin}deg)",
        f64::from(edit.straighten) / 10.0
    )
}

/// Why the edit can't be saved yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    NoChange,
    BadTime,
    TooShort,
}

impl Problem {
    fn message(self) -> String {
        match self {
            Problem::NoChange => t!("editor-no-change"),
            Problem::BadTime => t!("editor-bad-time"),
            Problem::TooShort => t!("editor-too-short"),
        }
    }
}

impl From<EditError> for Problem {
    fn from(e: EditError) -> Problem {
        match e {
            EditError::NoChange => Problem::NoChange,
            EditError::TrimTooShort => Problem::TooShort,
            _ => Problem::BadTime,
        }
    }
}

/// "1.5" for 1 500 ms, for the trim fields.
fn seconds_text(ms: i64) -> String {
    let tenths = (ms.max(0) + 50) / 100;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Reads a trim field ("1.5", "1,5", "0:01.5" or "1:02:03") as milliseconds.
fn parse_seconds(text: &str) -> Option<i64> {
    let text = text.trim().replace(',', ".");
    if text.is_empty() {
        return None;
    }
    let mut total = 0.0_f64;
    for part in text.split(':') {
        let v: f64 = part.parse().ok()?;
        if !v.is_finite() || v < 0.0 {
            return None;
        }
        total = total * 60.0 + v;
    }
    let ms = (total * 1000.0).round();
    // Longer than a year is not a real video position.
    (ms <= 31_536_000_000.0).then_some(ms as i64)
}

/// The video's playback position, in milliseconds.
async fn current_time() -> Option<i64> {
    let secs = document::eval(
        "const v = document.getElementById('edit-video'); return v ? v.currentTime : null;",
    )
    .await
    .ok()?
    .as_f64()?;
    secs.is_finite().then(|| (secs * 1000.0).round() as i64)
}

/// Undo, redo, save and close from the keyboard, wherever the focus is. Typing
/// in a field keeps its own undo, and Escape shuts the save menu before the
/// editor.
///
/// It also lets a mouse move the proportion chips when they run off the side:
/// the wheel scrolls them, and so does a drag, which then doesn't count as a
/// click on the chip it started on.
const KEYS: &str = r#"
const off = () => {
  if (window.__sbEditKeys) window.removeEventListener('keydown', window.__sbEditKeys, true);
  if (window.__sbEditWheel) window.removeEventListener('wheel', window.__sbEditWheel, true);
  if (window.__sbEditDrag) for (const t of ['pointerdown', 'pointermove', 'pointerup', 'click'])
    window.removeEventListener(t, window.__sbEditDrag, true);
};
off();
window.__sbEditKeys = (e) => {
  const t = e.target;
  const typing = t && (t.isContentEditable || /^(TEXTAREA|SELECT)$/.test(t.tagName)
    || (t.tagName === 'INPUT' && !/^(range|checkbox|button)$/.test(t.type)));
  const k = e.key.toLowerCase();
  let send = null;
  if ((e.ctrlKey || e.metaKey) && !e.altKey) {
    if (k === 'z') send = e.shiftKey ? 'redo' : 'undo';
    else if (k === 'y') send = 'redo';
    else if (k === 's' || k === 'enter') send = 'save';
  } else if (k === 'escape') send = document.querySelector('.editor .save-menu') ? 'menu' : 'close';
  if (!send || (typing && (send === 'undo' || send === 'redo'))) return;
  e.preventDefault();
  e.stopPropagation();
  dioxus.send(send);
};
const strip = (e) => {
  const s = e.target && e.target.closest && e.target.closest('.editor .ratios');
  return s && s.scrollWidth > s.clientWidth ? s : null;
};
window.__sbEditWheel = (e) => {
  const s = strip(e);
  if (!s || e.ctrlKey) return;
  const d = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
  if (!d) return;
  e.preventDefault();
  s.scrollLeft += d * (e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? s.clientWidth : 1);
};
let drag = null;
window.__sbEditDrag = (e) => {
  if (e.type === 'pointerdown') {
    const s = e.pointerType === 'mouse' && e.button === 0 ? strip(e) : null;
    drag = s ? { s, x: e.clientX, left: s.scrollLeft, moved: false } : null;
  } else if (e.type === 'pointermove') {
    if (!drag || !(e.buttons & 1)) return;
    const dx = e.clientX - drag.x;
    if (!drag.moved && Math.abs(dx) < 6) return;
    drag.moved = true;
    drag.s.scrollLeft = drag.left - dx;
  } else if (e.type === 'pointerup') {
    if (drag && !drag.moved) drag = null;
  } else if (drag) {
    e.preventDefault();
    e.stopPropagation();
    drag = null;
  }
};
window.addEventListener('keydown', window.__sbEditKeys, true);
window.addEventListener('wheel', window.__sbEditWheel, { capture: true, passive: false });
for (const t of ['pointerdown', 'pointermove', 'pointerup', 'click'])
  window.addEventListener(t, window.__sbEditDrag, true);
window.__sbEditOff = off;
"#;
const KEYS_OFF: &str = r#"
if (window.__sbEditOff) window.__sbEditOff();
window.__sbEditKeys = window.__sbEditWheel = window.__sbEditDrag = window.__sbEditOff = null;
"#;

/// What the editor saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Saved {
    pub id: i64,
    /// The original went to the recycle bin.
    pub binned: bool,
}

/// The editor for `post`, over the whole window.
#[component]
pub fn EditorSheet(
    post: Post,
    kind: EditKind,
    file_url: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<Saved>,
) -> Element {
    let width = post.width.and_then(|w| u32::try_from(w).ok()).unwrap_or(0);
    let height = post.height.and_then(|h| u32::try_from(h).ok()).unwrap_or(0);
    let duration = post.duration_ms.unwrap_or(0);
    let mut shape = use_signal(|| Shape::new(width, height));
    let mut history = use_signal(History::default);
    // Where a drag or a slide started, to make one step of it.
    let mut live_from = use_signal(|| None::<Shape>);
    let mut spin = use_signal(|| 0_i32);
    let mut busy = use_signal(|| false);
    let mut closing = use_signal(|| false);
    let mut menu = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut start = use_signal(|| seconds_text(0));
    let mut end = use_signal(move || seconds_text(duration));
    let keep = PREFS.read().keep_original;

    // Every change goes through here, so the picture turns the short way.
    let mut show = move |next: Shape| {
        let s = spun(*spin.peek(), next.edit.rotate);
        if s != *spin.peek() {
            spin.set(s);
        }
        shape.set(next);
    };
    let mut finish_live = move || {
        if let Some(before) = live_from.write().take()
            && before != *shape.peek()
        {
            history.write().record(before);
        }
    };
    let mut commit = move |next: Shape| {
        finish_live();
        let now = shape.peek().clone();
        if next != now {
            history.write().record(now);
            show(next);
        }
    };
    let mut live = move |next: Shape| {
        if live_from.peek().is_none() {
            live_from.set(Some(shape.peek().clone()));
        }
        show(next);
    };
    let mut undo = move || {
        finish_live();
        let now = shape.peek().clone();
        let back = history.write().undo(&now);
        if let Some(back) = back {
            show(back);
        }
    };
    let mut redo = move || {
        finish_live();
        let now = shape.peek().clone();
        let ahead = history.write().redo(&now);
        if let Some(ahead) = ahead {
            show(ahead);
        }
    };

    // The request the editor describes, or why it can't be sent.
    let request = move || -> Result<MediaEdit, Problem> {
        let e = match kind {
            EditKind::Image => shape.read().request(width, height),
            EditKind::Video => {
                let (Some(s), Some(t)) = (parse_seconds(&start()), parse_seconds(&end())) else {
                    return Err(Problem::BadTime);
                };
                MediaEdit {
                    trim: Some(Trim {
                        start_ms: s,
                        end_ms: t.min(duration),
                    }),
                    ..MediaEdit::default()
                }
            }
        };
        e.check(kind, width, height, post.duration_ms)
            .map(|()| e)
            .map_err(Problem::from)
    };
    let id = post.id;
    let mut close = move || {
        if busy() || closing() {
            return;
        }
        closing.set(true);
        spawn(async move {
            crate::activity::wait_ms(180).await;
            onclose.call(());
        });
    };
    let mut save = move || {
        if busy() {
            return;
        }
        let Ok(mut body) = request() else {
            return;
        };
        let Some(client) = store::client() else {
            return;
        };
        body.bin_original = !PREFS.peek().keep_original;
        let binned = body.bin_original;
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.edit_post(id, &body).await {
                Ok(saved) => onsaved.call(Saved {
                    id: saved.id,
                    binned,
                }),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    use_future(move || async move {
        let mut keys = document::eval(KEYS);
        while let Ok(key) = keys.recv::<String>().await {
            match key.as_str() {
                "undo" if kind == EditKind::Image => undo(),
                "redo" if kind == EditKind::Image => redo(),
                "save" => save(),
                "close" => close(),
                "menu" => menu.set(false),
                _ => {}
            }
        }
    });
    use_drop(|| {
        document::eval(KEYS_OFF);
    });

    let problem = request().err();
    let changed = problem != Some(Problem::NoChange);
    let (can_undo, can_redo) = {
        let h = history.read();
        (!h.past.is_empty(), !h.future.is_empty())
    };
    let tool = match kind {
        EditKind::Image => (Icon::Crop, t!("editor-crop")),
        EditKind::Video => (Icon::Cut, t!("editor-trim")),
    };
    let title = match kind {
        EditKind::Image => t!("editor-title-image"),
        EditKind::Video => t!("editor-title-video"),
    };

    rsx! {
        div {
            // `live` while a drag or a slide is under way, so nothing eases behind the finger.
            class: match (closing(), live_from.read().is_some()) {
                (true, _) => "editor closing",
                (false, true) => "editor live",
                (false, false) => "editor",
            },
            role: "dialog",
            aria_modal: "true",
            aria_label: "{title}",
            div { class: "editor-top",
                button { class: "editor-pill glass float icon-only", r#type: "button",
                    aria_label: t!("share-close"), title: t!("share-close"),
                    disabled: busy(),
                    onclick: move |_| close(),
                    Ico { icon: Icon::Close }
                }
                if kind == EditKind::Image {
                    div { class: "editor-pill glass float editor-history", role: "group", aria_label: t!("editor-history"),
                        button { class: "icon-btn", r#type: "button", disabled: !can_undo,
                            aria_label: t!("editor-undo"), title: t!("editor-undo-keys"),
                            onclick: move |_| undo(),
                            Ico { icon: Icon::Undo }
                        }
                        button { class: "icon-btn", r#type: "button", disabled: !can_redo,
                            aria_label: t!("editor-redo"), title: t!("editor-redo-keys"),
                            onclick: move |_| redo(),
                            Ico { icon: Icon::Redo }
                        }
                        button { class: "icon-btn", r#type: "button", disabled: !changed,
                            aria_label: t!("editor-reset"), title: t!("editor-reset"),
                            onclick: move |_| commit(Shape::new(width, height)),
                            Ico { icon: Icon::Restore }
                        }
                    }
                }
                // Save, and beside it a menu for what happens to the original.
                div { class: "editor-save-group",
                    div { class: "editor-save-split",
                        button { class: "btn editor-save", r#type: "button",
                            disabled: busy() || problem.is_some(),
                            title: t!("editor-save-keys"),
                            onclick: move |_| save(),
                            if busy() {
                                span { class: "spinner small" }
                            } else {
                                Ico { icon: Icon::Check }
                            }
                            span { if keep { {t!("editor-save-copy")} } else { {t!("editor-save")} } }
                        }
                        button { class: "btn editor-save-more", r#type: "button",
                            disabled: busy(),
                            aria_label: t!("editor-save-options"), title: t!("editor-save-options"),
                            aria_haspopup: "menu",
                            aria_expanded: if menu() { "true" } else { "false" },
                            onclick: move |_| menu.toggle(),
                            Ico { icon: Icon::ChevronRight, size: 20 }
                        }
                    }
                    if menu() {
                        div { class: "select-backdrop", onclick: move |_| menu.set(false) }
                        div { class: "save-menu", role: "menu", aria_label: t!("editor-save-options"),
                            for (keeps, icon, label, hint) in [
                                (false, Icon::Delete, t!("editor-save-replace"), t!("editor-save-replace-hint")),
                                (true, Icon::Copy, t!("editor-keep-original"), t!("editor-keep-original-hint")),
                            ] {
                                button {
                                    key: "{keeps}",
                                    class: if keep == keeps { "save-option on" } else { "save-option" },
                                    r#type: "button",
                                    role: "menuitemradio",
                                    aria_checked: if keep == keeps { "true" } else { "false" },
                                    title: "{hint}",
                                    onclick: move |_| {
                                        PREFS.write().keep_original = keeps;
                                        menu.set(false);
                                    },
                                    Ico { icon, size: 20 }
                                    span { "{label}" }
                                    span { class: "save-option-check", Ico { icon: Icon::Check, size: 20 } }
                                }
                            }
                        }
                    }
                }
            }
            match kind {
                EditKind::Image => rsx! {
                    ImageCanvas {
                        src: file_url.clone(),
                        shape,
                        spin: spin(),
                        width,
                        height,
                        onlive: move |next: Shape| live(next),
                        onsettle: move |()| finish_live(),
                    }
                },
                EditKind::Video => rsx! {
                    div { class: "editor-canvas",
                        video { id: "edit-video", class: "editor-video", src: "{file_url}", controls: true, playsinline: true, preload: "metadata" }
                    }
                },
            }
            div { class: "editor-panel glass float",
                div { class: "tool-tabs", role: "tablist", aria_label: t!("editor-tools"),
                    button { class: "tool-tab on", r#type: "button", role: "tab", aria_selected: "true",
                        Ico { icon: tool.0 }
                        span { "{tool.1}" }
                    }
                }
                div { class: "tool-body", role: "tabpanel",
                    match kind {
                        EditKind::Image => rsx! {
                            CropTools {
                                shape,
                                width,
                                height,
                                oncommit: move |next: Shape| commit(next),
                                onlive: move |next: Shape| live(next),
                                onsettle: move |()| finish_live(),
                            }
                        },
                        EditKind::Video => rsx! {
                            div { class: "trim-fields",
                                TrimField {
                                    id: "trim-start",
                                    label: t!("editor-start"),
                                    value: start,
                                    onnow: move |ms: i64| start.set(seconds_text(ms)),
                                }
                                TrimField {
                                    id: "trim-end",
                                    label: t!("editor-end"),
                                    value: end,
                                    onnow: move |ms: i64| end.set(seconds_text(ms.min(duration))),
                                }
                            }
                            p { class: "muted hint",
                                {
                                    let keeps = match (parse_seconds(&start()), parse_seconds(&end())) {
                                        (Some(s), Some(e)) if e > s => format_duration(e.min(duration) - s),
                                        _ => None,
                                    };
                                    match keeps {
                                        Some(k) => t!("editor-keeps", length = k, total = format_duration(duration).unwrap_or_default()),
                                        None => t!("editor-bad-time"),
                                    }
                                }
                                " "
                                {t!("editor-keyframe-hint")}
                            }
                        },
                    }
                    if let Some(message) = error() {
                        ErrorBanner { message }
                    } else if let Some(why) = problem.filter(|p| changed && *p != Problem::NoChange) {
                        p { class: "field-error", role: "status", {why.message()} }
                    }
                }
            }
        }
    }
}

/// Hands `oncommit` the shape `f` makes from the current one. The read
/// ends before the handler runs: the handler writes the same signal, and
/// a read still held in the call would make that write panic.
fn change(shape: Signal<Shape>, oncommit: EventHandler<Shape>, f: impl FnOnce(&Shape) -> Shape) {
    let next = f(&shape.peek());
    oncommit.call(next);
}

/// A proportion as a rounded frame, drawn in the icons' 2-unit line. The
/// frame eases to its new size when the proportion turns.
#[component]
fn RatioGlyph(w: f64, h: f64) -> Element {
    let k = 16.0 / w.max(h).max(1.0);
    let (w, h) = (w * k, h * k);
    let (x, y) = (12.0 - w / 2.0, 12.0 - h / 2.0);
    rsx! {
        svg { class: "icon ratio-glyph", width: "24", height: "24", view_box: "0 0 24 24", "aria-hidden": "true",
            rect {
                x: "{x:.2}", y: "{y:.2}", width: "{w:.2}", height: "{h:.2}", rx: "2.5",
                style: "x: {x:.2}px; y: {y:.2}px; width: {w:.2}px; height: {h:.2}px",
                fill: "none", stroke: "currentColor", stroke_width: "2",
            }
        }
    }
}

/// Proportions, straightening, turning and flipping.
#[component]
fn CropTools(
    shape: Signal<Shape>,
    width: u32,
    height: u32,
    oncommit: EventHandler<Shape>,
    onlive: EventHandler<Shape>,
    onsettle: EventHandler<()>,
) -> Element {
    let now = shape();
    let mut custom_open = use_signal(|| false);
    let mut custom_w = use_signal(|| "16".to_string());
    let mut custom_h = use_signal(|| "10".to_string());
    let custom_on = custom_open() || !now.aspect.is_preset();
    let apply_custom = move || {
        if let (Some(w), Some(h)) = (ratio_side(&custom_w()), ratio_side(&custom_h())) {
            change(shape, oncommit, |s| {
                s.with_aspect(Aspect::Fixed(w, h), width, height)
            });
        }
    };
    let mut pick = move |aspect: Aspect| {
        custom_open.set(false);
        change(shape, oncommit, |s| s.with_aspect(aspect, width, height));
    };
    let (fw, fh) = now.frame(width, height);
    let degrees = f64::from(now.edit.straighten) / 10.0;
    let max = f64::from(MAX_STRAIGHTEN) / 10.0;
    rsx! {
        div { class: "tool-group ratios", role: "group", aria_label: t!("editor-ratio"),
            button {
                class: if now.aspect == Aspect::Free && !custom_on { "ratio on" } else { "ratio" },
                r#type: "button", aria_pressed: if now.aspect == Aspect::Free && !custom_on { "true" } else { "false" },
                onclick: move |_| pick(Aspect::Free),
                Ico { icon: Icon::RatioFree }
                span { {t!("editor-ratio-free")} }
            }
            button {
                class: if now.aspect == Aspect::Original && !custom_on { "ratio on" } else { "ratio" },
                r#type: "button", aria_pressed: if now.aspect == Aspect::Original && !custom_on { "true" } else { "false" },
                onclick: move |_| pick(Aspect::Original),
                RatioGlyph { w: fw, h: fh }
                span { {t!("editor-ratio-original")} }
            }
            for (a, b) in RATIOS {
                {
                    let (w, h) = if now.wide { (a.max(b), a.min(b)) } else { (a.min(b), a.max(b)) };
                    let on = now.aspect == Aspect::Fixed(w, h) && !custom_on;
                    rsx! {
                        button {
                            key: "{a}x{b}",
                            class: if on { "ratio on" } else { "ratio" },
                            r#type: "button", aria_pressed: if on { "true" } else { "false" },
                            onclick: move |_| pick(Aspect::Fixed(w, h)),
                            RatioGlyph { w: f64::from(w), h: f64::from(h) }
                            span { class: "num", "{w}:{h}" }
                        }
                    }
                }
            }
            button {
                class: if custom_on { "ratio on" } else { "ratio" },
                r#type: "button", aria_pressed: if custom_on { "true" } else { "false" },
                aria_expanded: if custom_open() { "true" } else { "false" },
                onclick: move |_| {
                    if let Aspect::Fixed(w, h) = shape.peek().aspect {
                        custom_w.set(w.to_string());
                        custom_h.set(h.to_string());
                    }
                    custom_open.set(!custom_open());
                },
                Ico { icon: Icon::RatioCustom }
                span { {t!("editor-ratio-custom")} }
            }
            button { class: "icon-btn swap", r#type: "button",
                aria_label: t!("editor-ratio-swap"), title: t!("editor-ratio-swap"),
                onclick: move |_| change(shape, oncommit, |s| s.swap_orientation(width, height)),
                span { class: if now.wide { "swap-icon wide" } else { "swap-icon" }, Ico { icon: Icon::Orientation } }
            }
        }
        if custom_open() {
            div { class: "custom-ratio",
                input {
                    r#type: "number", inputmode: "numeric", min: "1", max: "{MAX_RATIO_SIDE}",
                    aria_label: t!("editor-ratio-width"),
                    aria_invalid: if ratio_side(&custom_w()).is_none() { "true" } else { "false" },
                    value: "{custom_w}",
                    oninput: move |e| {
                        custom_w.set(e.value());
                        apply_custom();
                    },
                }
                span { aria_hidden: "true", ":" }
                input {
                    r#type: "number", inputmode: "numeric", min: "1", max: "{MAX_RATIO_SIDE}",
                    aria_label: t!("editor-ratio-height"),
                    aria_invalid: if ratio_side(&custom_h()).is_none() { "true" } else { "false" },
                    value: "{custom_h}",
                    oninput: move |e| {
                        custom_h.set(e.value());
                        apply_custom();
                    },
                }
            }
        }
        div { class: "tool-group dial", role: "group", aria_label: t!("editor-straighten"),
            button { class: if now.edit.straighten == 0 { "dial-value" } else { "dial-value set" }, r#type: "button",
                title: t!("editor-straighten-reset"), aria_label: t!("editor-straighten-reset"),
                onclick: move |_| {
                    let mut next = shape.peek().clone();
                    next.edit.straighten = 0;
                    oncommit.call(next);
                },
                "{degrees:.1}°"
            }
            div { class: "dial-track", style: "--at: {(degrees + max) / (2.0 * max):.4}",
                input {
                    r#type: "range",
                    min: "-{max}", max: "{max}", step: "0.1",
                    aria_label: t!("editor-straighten"),
                    value: "{degrees:.1}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            let tenths = within((v * 10.0).round(), -f64::from(MAX_STRAIGHTEN), f64::from(MAX_STRAIGHTEN));
                            let mut next = shape.peek().clone();
                            // Within ±450, so the cast is exact.
                            next.edit.straighten = tenths as i16;
                            onlive.call(next);
                        }
                    },
                    onchange: move |_| onsettle.call(()),
                }
            }
        }
        div { class: "tool-group turns", role: "group", aria_label: t!("editor-turn-flip"),
            button { class: "icon-btn", r#type: "button", title: t!("editor-turn-left"), aria_label: t!("editor-turn-left"),
                onclick: move |_| change(shape, oncommit, |s| s.turn(false)),
                Ico { icon: Icon::RotateLeft }
            }
            button { class: "icon-btn", r#type: "button", title: t!("editor-turn-right"), aria_label: t!("editor-turn-right"),
                onclick: move |_| change(shape, oncommit, |s| s.turn(true)),
                Ico { icon: Icon::RotateRight }
            }
            button { class: "icon-btn", r#type: "button", title: t!("editor-flip-h"), aria_label: t!("editor-flip-h"),
                onclick: move |_| change(shape, oncommit, |s| s.flip(true)),
                Ico { icon: Icon::Flip }
            }
            button { class: "icon-btn flip-v", r#type: "button", title: t!("editor-flip-v"), aria_label: t!("editor-flip-v"),
                onclick: move |_| change(shape, oncommit, |s| s.flip(false)),
                Ico { icon: Icon::Flip }
            }
        }
    }
}

#[component]
fn TrimField(
    id: &'static str,
    label: String,
    value: Signal<String>,
    onnow: EventHandler<i64>,
) -> Element {
    let mut value = value;
    let invalid = parse_seconds(&value()).is_none();
    rsx! {
        div { class: "field",
            label { r#for: id, "{label}" }
            div { class: "trim-row",
                input {
                    id,
                    inputmode: "decimal",
                    spellcheck: "false",
                    aria_invalid: if invalid { "true" } else { "false" },
                    value: "{value}",
                    oninput: move |e| value.set(e.value()),
                }
                button { class: "btn tonal small", r#type: "button", title: t!("editor-now-title"),
                    onclick: move |_| {
                        spawn(async move {
                            if let Some(ms) = current_time().await {
                                onnow.call(ms.max(0));
                            }
                        });
                    },
                    {t!("editor-now")}
                }
            }
        }
    }
}

/// A drag in progress: what is dragged, from where, and the box it started with.
#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    from: ClientPoint,
    start: CropBox,
}

/// The picture as it will be saved, with the crop box over it. The room
/// around it takes the pointer too, so a drag can go past the picture.
#[component]
fn ImageCanvas(
    src: String,
    shape: Signal<Shape>,
    spin: i32,
    width: u32,
    height: u32,
    onlive: EventHandler<Shape>,
    onsettle: EventHandler<()>,
) -> Element {
    let mut room = use_signal(|| None::<(f64, f64)>);
    let mut drag = use_signal(|| None::<Drag>);
    let now = shape();
    let (fw, fh) = now.frame(width, height);
    let size = room().map(|r| frame_size(fw, fh, r));
    let ratio = now.aspect.ratio(fw, fh);
    let mut begin = move |handle: Handle, e: PointerEvent| {
        e.stop_propagation();
        e.prevent_default();
        if !e.is_primary() {
            return;
        }
        drag.set(Some(Drag {
            handle,
            from: e.client_coordinates(),
            start: shape.peek().crop,
        }));
    };
    let mut end_drag = move || {
        if drag.peek().is_some() {
            drag.set(None);
            onsettle.call(());
        }
    };
    let dragging = drag().is_some();
    let pixels = {
        let (w, h) = now.edit.turned_size(width, height);
        now.crop
            .pixels(w, h)
            .map_or((w, h), |c| (c.width, c.height))
    };
    rsx! {
        div {
            class: if dragging { "editor-canvas dragging" } else { "editor-canvas" },
            "data-pointer": crate::bridge::ALL,
            onresize: move |e| {
                if let Ok(s) = e.get_content_box_size() {
                    room.set(Some((s.width, s.height)));
                }
            },
            onpointermove: move |e| {
                let (Some(d), Some((w, h))) = (drag(), size) else {
                    return;
                };
                if !e.is_primary() {
                    return;
                }
                let p = e.client_coordinates();
                // Pointer pixels over frame pixels, as picture pixels.
                let (dx, dy) = ((p.x - d.from.x) * fw / w, (p.y - d.from.y) * fh / h);
                let mut next = shape.peek().clone();
                next.crop = d.start.dragged(d.handle, dx, dy, fw, fh, ratio);
                onlive.call(next);
            },
            onpointerup: move |_| end_drag(),
            onpointercancel: move |_| end_drag(),
            onpointerleave: move |_| end_drag(),
            if let Some((w, h)) = size {
                div { class: "editor-frame", style: "width: {w:.2}px; height: {h:.2}px",
                    div { class: "editor-clip",
                        img {
                            src: "{src}", alt: "", draggable: "false",
                            style: picture_style(&now.edit, spin, (w, h), width, height),
                        }
                        div { class: "crop-shade", style: now.crop.style() }
                    }
                    div {
                        class: "crop-box",
                        style: now.crop.style(),
                        onpointerdown: move |e| begin(Handle::Move, e),
                        span { class: "crop-grid" }
                        for (handle, name) in Handle::ALL {
                            span {
                                key: "{name}",
                                class: "crop-handle {name}",
                                onpointerdown: move |e| begin(handle, e),
                            }
                        }
                        span { class: "crop-size num", "{pixels.0} × {pixels.1}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: CropBox, b: CropBox) -> bool {
        [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)]
            .iter()
            .all(|(p, q)| (p - q).abs() < 1e-9)
    }

    fn b(x: f64, y: f64, w: f64, h: f64) -> CropBox {
        CropBox { x, y, w, h }
    }

    #[test]
    fn limits_never_panic() {
        assert_eq!(within(5.0, 0.0, 1.0), 1.0);
        assert_eq!(within(-5.0, 0.0, 1.0), 0.0);
        // `f64::clamp` panics on both of these.
        assert_eq!(within(0.5, 11.0, 10.0), 11.0);
        assert_eq!(within(f64::NAN, 0.0, 1.0), 0.0);
        let odd = b(f64::NAN, 2.0, -1.0, f64::INFINITY);
        for (handle, _) in Handle::ALL {
            let _ = odd
                .dragged(handle, f64::NAN, 1.0, 10.0, 10.0, None)
                .pixels(10, 10);
            let _ = odd.dragged(handle, 1.0, 1.0, 10.0, 10.0, Some(f64::NAN));
            let _ = odd.dragged(handle, 1.0, 1.0, 0.0, 0.0, Some(2.0));
        }
        let _ = odd.pixels(0, 0);
        let _ = odd.with_ratio(0.0, 10.0, 10.0);
        assert_eq!(frame_size(0.0, 0.0, (f64::NAN, 5.0)), (1.0, 1.0));
    }

    #[test]
    fn drags_free_from_any_side() {
        let (fw, fh) = (100.0, 100.0);
        let full = CropBox::FULL;
        let d = |c: CropBox, h, dx, dy| c.dragged(h, dx, dy, fw, fh, None);
        assert!(close(
            d(full, Handle::TopLeft, 25.0, 50.0),
            b(0.25, 0.5, 0.75, 0.5)
        ));
        // Past the edge or the other side, it stops.
        assert!(close(
            d(full, Handle::BottomRight, 50.0, -200.0),
            b(0.0, 0.0, 1.0, MIN_CROP)
        ));
        assert!(close(d(full, Handle::TopRight, 300.0, 0.0), full));
        // A side moves only itself.
        assert!(close(
            d(full, Handle::Left, 30.0, 40.0),
            b(0.3, 0.0, 0.7, 1.0)
        ));
        assert!(close(
            d(full, Handle::Bottom, 30.0, -40.0),
            b(0.0, 0.0, 1.0, 0.6)
        ));
        assert!(close(
            d(full, Handle::Top, 0.0, 10.0),
            b(0.0, 0.1, 1.0, 0.9)
        ));
        assert!(close(
            d(full, Handle::Right, -10.0, 0.0),
            b(0.0, 0.0, 0.9, 1.0)
        ));
        let small = b(0.2, 0.2, 0.5, 0.5);
        assert!(close(
            d(small, Handle::Move, 60.0, -60.0),
            b(0.5, 0.0, 0.5, 0.5)
        ));
        assert!(close(
            d(small, Handle::BottomLeft, -10.0, 10.0),
            b(0.1, 0.2, 0.6, 0.6)
        ));
    }

    #[test]
    fn drags_keep_the_proportions() {
        // A 200 × 100 picture with a 1:1 crop.
        let (fw, fh) = (200.0, 100.0);
        let square = CropBox::FULL.with_ratio(1.0, fw, fh);
        assert!(close(square, b(0.25, 0.0, 0.5, 1.0)));
        let px = |c: CropBox| (c.w * fw, c.h * fh);
        let same = |c: CropBox| {
            let (w, h) = px(c);
            (w - h).abs() < 1e-6
        };
        // Every handle, every way: still square and inside.
        for (handle, _) in Handle::ALL {
            for (dx, dy) in [(-30.0, -10.0), (25.0, 40.0), (-500.0, 500.0), (5.0, -3.0)] {
                let c = square.dragged(handle, dx, dy, fw, fh, Some(1.0));
                assert!(same(c), "{handle:?} {dx} {dy}: {c:?}");
                assert!(c.x >= -1e-9 && c.y >= -1e-9, "{handle:?}: {c:?}");
                assert!(
                    c.x + c.w <= 1.0 + 1e-9 && c.y + c.h <= 1.0 + 1e-9,
                    "{handle:?}: {c:?}"
                );
            }
        }
        // A corner grows the way the pointer went further, from the other corner.
        let near =
            |(w, h): (f64, f64), want: f64| (w - want).abs() < 1e-9 && (h - want).abs() < 1e-9;
        let c = square.dragged(Handle::BottomRight, -20.0, -40.0, fw, fh, Some(1.0));
        assert!(near(px(c), 80.0), "{c:?}");
        assert!((c.x * fw - 50.0).abs() < 1e-9 && c.y == 0.0, "{c:?}");
        // A side keeps the box's middle on the other axis.
        let c = square.dragged(Handle::Left, 40.0, 0.0, fw, fh, Some(1.0));
        assert!(near(px(c), 60.0), "{c:?}");
        assert!((c.y * fh - 20.0).abs() < 1e-9, "{c:?}");
        // Moving keeps the size.
        let c = square.dragged(Handle::Move, 70.0, 0.0, fw, fh, Some(1.0));
        assert!((c.x - 0.5).abs() < 1e-9 && same(c), "{c:?}");
    }

    #[test]
    fn proportions_fit_the_picture() {
        // 3:4 in a landscape 400 × 300 frame takes the full height.
        let c = CropBox::FULL.with_ratio(0.75, 400.0, 300.0);
        assert!(
            (c.h - 1.0).abs() < 1e-9 && (c.w * 400.0 - 225.0).abs() < 1e-9,
            "{c:?}"
        );
        // Centred on the old crop, but pushed back inside.
        let c = b(0.8, 0.0, 0.2, 0.2).with_ratio(1.0, 100.0, 100.0);
        assert!(close(c, CropBox::FULL));
        let mut shape = Shape::new(400, 300);
        assert!(shape.wide);
        shape = shape.with_aspect(Aspect::Fixed(4, 3), 400, 300);
        assert!(close(shape.crop, CropBox::FULL));
        assert_eq!(shape.request(400, 300).crop, None);
        // The other way up, then turned: the turn takes the crop along.
        shape = shape.swap_orientation(400, 300);
        assert_eq!(shape.aspect, Aspect::Fixed(3, 4));
        assert!(!shape.wide);
        let turned = shape.turn(true);
        assert_eq!(turned.aspect, Aspect::Fixed(4, 3));
        let (w, h) = turned.frame(400, 300);
        assert_eq!((w, h), (300.0, 400.0));
        let crop = turned.request(400, 300).crop.unwrap();
        assert_eq!((crop.width, crop.height), (300, 225));
        assert_eq!(Aspect::Original.ratio(300.0, 400.0), Some(0.75));
        assert!(Aspect::Fixed(16, 9).is_preset() && Aspect::Fixed(9, 16).is_preset());
        assert!(!Aspect::Fixed(5, 7).is_preset());
        assert_eq!(ratio_side(" 21 "), Some(21));
        assert_eq!(ratio_side("0"), None);
        assert_eq!(ratio_side("101"), None);
        assert_eq!(ratio_side("x"), None);
    }

    #[test]
    fn turns_and_flips_take_the_crop_along() {
        let c = b(0.1, 0.2, 0.3, 0.4);
        assert!(close(c.turned(true), b(0.4, 0.1, 0.4, 0.3)));
        assert!(close(c.turned(true).turned(false), c));
        let four = c.turned(true).turned(true).turned(true).turned(true);
        assert!(close(four, c));
        assert!(close(c.mirrored(true), b(0.6, 0.2, 0.3, 0.4)));
        assert!(close(c.mirrored(true).mirrored(true), c));
        // Mirroring leans a straightened picture the other way.
        let mut shape = Shape::new(10, 10);
        shape.edit.straighten = 120;
        let flipped = shape.flip(true);
        assert_eq!(flipped.edit.straighten, -120);
        assert!(flipped.edit.flip_h);
        assert_eq!(flipped.flip(true), shape);
    }

    #[test]
    fn undo_and_redo() {
        let a = Shape::new(10, 10);
        let b = a.turn(true);
        let c = b.flip(true);
        let mut h = History::default();
        h.record(a.clone());
        h.record(b.clone());
        assert_eq!(h.undo(&c), Some(b.clone()));
        assert_eq!(h.undo(&b), Some(a.clone()));
        assert_eq!(h.undo(&a), None);
        assert_eq!(h.redo(&a), Some(b.clone()));
        // A new change forgets what was undone.
        h.record(b.clone());
        assert_eq!(h.redo(&c), None);
        for _ in 0..MAX_HISTORY + 20 {
            h.record(a.clone());
        }
        assert_eq!(h.past.len(), MAX_HISTORY);
    }

    #[test]
    fn turns_the_short_way() {
        assert_eq!(spun(0, 90), 90);
        assert_eq!(spun(270, 0), 360);
        assert_eq!(spun(360, 270), 270);
        assert_eq!(spun(-90, 180), -180);
        assert_eq!(spun(0, 180), 180);
    }

    #[test]
    fn fits_the_frame_and_previews_turns() {
        assert_eq!(frame_size(400.0, 200.0, (840.0, 840.0)), (800.0, 400.0));
        assert_eq!(frame_size(200.0, 400.0, (840.0, 440.0)), (200.0, 400.0));
        let mut e = MediaEdit::default();
        let style = picture_style(&e, 0, (400.0, 200.0), 400, 200);
        assert!(
            style.starts_with("width: 400.00px; height: 200.00px"),
            "{style}"
        );
        e.turn(true);
        e.flip_h = true;
        e.straighten = -50;
        let style = picture_style(&e, 90, (100.0, 200.0), 400, 200);
        assert!(
            style.starts_with("width: 200.00px; height: 100.00px"),
            "{style}"
        );
        assert!(style.contains("rotate(-5.0deg)"), "{style}");
        assert!(style.ends_with("scale(-1, 1) rotate(90deg)"), "{style}");
    }

    #[test]
    fn crops_in_pixels() {
        assert_eq!(CropBox::FULL.pixels(40, 30), None);
        assert_eq!(
            b(0.25, 0.0, 0.5, 1.0).pixels(40, 30),
            Some(Crop {
                x: 10,
                y: 0,
                width: 20,
                height: 30
            })
        );
        // Always at least one pixel, and inside the picture.
        let c = b(0.999, 0.999, 0.0001, 0.0001).pixels(10, 10).unwrap();
        assert!(c.width >= 1 && c.x + c.width <= 10, "{c:?}");
        assert!(c.height >= 1 && c.y + c.height <= 10, "{c:?}");
        // What the server accepts.
        let e = MediaEdit {
            crop: Some(c),
            ..MediaEdit::default()
        };
        assert!(e.check(EditKind::Image, 10, 10, None).is_ok());
    }

    #[test]
    fn reads_trim_times() {
        assert_eq!(parse_seconds("1.5"), Some(1_500));
        assert_eq!(parse_seconds(" 1,25 "), Some(1_250));
        assert_eq!(parse_seconds("0:01.5"), Some(1_500));
        assert_eq!(parse_seconds("1:02:03"), Some(3_723_000));
        assert_eq!(parse_seconds(""), None);
        assert_eq!(parse_seconds("-1"), None);
        assert_eq!(parse_seconds("abc"), None);
        assert_eq!(parse_seconds("inf"), None);
        assert_eq!(parse_seconds("1e300"), None);
        assert_eq!(seconds_text(1_500), "1.5");
        assert_eq!(seconds_text(0), "0.0");
        assert_eq!(seconds_text(61_049), "61.0");
    }

    /// A button that turned the picture crashed the app: the handler
    /// wrote the shape while the button still held a read of it.
    #[test]
    fn a_change_lets_the_handler_write_the_shape() {
        use std::cell::Cell;
        use std::rc::Rc;
        thread_local! {
            static TURNED: Cell<Option<u16>> = const { Cell::new(None) };
        }
        fn app() -> Element {
            use_hook(|| {
                let mut shape = Signal::new(Shape::new(400, 300));
                let oncommit = EventHandler::new(move |next: Shape| shape.set(next));
                change(shape, oncommit, |s| s.turn(true));
                TURNED.with(|t| t.set(Some(shape.peek().edit.rotate)));
                Rc::new(())
            });
            rsx! {}
        }
        let mut dom = dioxus::core::VirtualDom::new(app);
        dom.rebuild_in_place();
        assert_eq!(TURNED.with(Cell::get), Some(90));
    }
}
