//! The media editor: turn, flip and crop an image, or trim a video. The
//! server saves the result as a new post; the original stays as it was
//! unless the user bins it.

use std::rc::Rc;

use dioxus::html::geometry::{ClientPoint, PixelsRect};
use dioxus::prelude::*;
use starbooru_core::{Crop, EditError, EditKind, MediaEdit, Post, Trim};

use super::ErrorBanner;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::format_duration;

/// Smallest crop side, as a share of the image side.
const MIN_CROP: f64 = 0.04;

/// `v` limited to `lo..=hi`, where `f64::clamp` would panic on `lo > hi`
/// or NaN; `lo` wins then, and NaN is `lo`.
fn within(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_nan() { lo } else { v.min(hi).max(lo) }
}

/// A crop as shares (0 to 1) of the turned and flipped image, so it doesn't
/// depend on how big the preview is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CropBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl CropBox {
    const FULL: CropBox = CropBox {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

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

    /// Moves `handle` by `dx`, `dy` (shares of the image), keeping the box
    /// inside the image and at least `MIN_CROP` wide and high.
    fn dragged(self, handle: Handle, dx: f64, dy: f64) -> CropBox {
        let (left, top) = (self.x, self.y);
        let (right, bottom) = (self.x + self.w, self.y + self.h);
        let (l, t, r, b) = match handle {
            Handle::Move => {
                let x = within(left + dx, 0.0, 1.0 - self.w);
                let y = within(top + dy, 0.0, 1.0 - self.h);
                return CropBox { x, y, ..self };
            }
            Handle::TopLeft => (
                within(left + dx, 0.0, right - MIN_CROP),
                within(top + dy, 0.0, bottom - MIN_CROP),
                right,
                bottom,
            ),
            Handle::TopRight => (
                left,
                within(top + dy, 0.0, bottom - MIN_CROP),
                within(right + dx, left + MIN_CROP, 1.0),
                bottom,
            ),
            Handle::BottomLeft => (
                within(left + dx, 0.0, right - MIN_CROP),
                top,
                right,
                within(bottom + dy, top + MIN_CROP, 1.0),
            ),
            Handle::BottomRight => (
                left,
                top,
                within(right + dx, left + MIN_CROP, 1.0),
                within(bottom + dy, top + MIN_CROP, 1.0),
            ),
        };
        CropBox {
            x: l,
            y: t,
            w: r - l,
            h: b - t,
        }
    }

    /// The crop in pixels of a `width` × `height` image; `None` when it
    /// keeps the whole image. Always at least one pixel, inside the image.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    Move,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Handle {
    const CORNERS: [(Handle, &'static str); 4] = [
        (Handle::TopLeft, "tl"),
        (Handle::TopRight, "tr"),
        (Handle::BottomLeft, "bl"),
        (Handle::BottomRight, "br"),
    ];
}

/// A drag in progress: what is dragged, from where, and the box it started with.
#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    from: ClientPoint,
    start: CropBox,
}

/// Styles for the preview: the frame has the turned image's shape, and the
/// image inside is sized so that turning it fills the frame.
fn preview_styles(edit: &MediaEdit, width: u32, height: u32) -> (String, String) {
    let (w, h) = edit.turned_size(width.max(1), height.max(1));
    let ratio = f64::from(w) / f64::from(h);
    let frame = format!("aspect-ratio: {w} / {h}; width: min(100%, calc(56dvh * {ratio:.4}))");
    let (iw, ih) = if edit.rotate % 180 == 90 {
        (100.0 / ratio, 100.0 * ratio)
    } else {
        (100.0, 100.0)
    };
    let sx = if edit.flip_h { -1 } else { 1 };
    let sy = if edit.flip_v { -1 } else { 1 };
    // Applied right to left: turn, then flip, then centre.
    let image = format!(
        "width: {iw:.3}%; height: {ih:.3}%; transform: translate(-50%, -50%) scale({sx}, {sy}) rotate({}deg)",
        edit.rotate
    );
    (frame, image)
}

/// Why the form can't be saved yet.
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

/// The editor sheet for `post`. `onsaved` gets the new post's id.
#[component]
pub fn EditorSheet(
    post: Post,
    kind: EditKind,
    file_url: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<i64>,
) -> Element {
    let mut edit = use_signal(MediaEdit::default);
    let mut crop = use_signal(|| CropBox::FULL);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let width = post.width.and_then(|w| u32::try_from(w).ok()).unwrap_or(0);
    let height = post.height.and_then(|h| u32::try_from(h).ok()).unwrap_or(0);
    let duration = post.duration_ms.unwrap_or(0);
    let mut start = use_signal(|| seconds_text(0));
    let mut end = use_signal(move || seconds_text(duration));

    // The request the form describes, or why it can't be sent.
    let request = move || -> Result<MediaEdit, Problem> {
        let mut e = edit();
        match kind {
            EditKind::Image => {
                let (w, h) = e.turned_size(width, height);
                e.crop = crop().pixels(w, h);
            }
            EditKind::Video => {
                let (Some(s), Some(t)) = (parse_seconds(&start()), parse_seconds(&end())) else {
                    return Err(Problem::BadTime);
                };
                e.trim = Some(Trim {
                    start_ms: s,
                    end_ms: t.min(duration),
                });
            }
        }
        e.check(kind, width, height, post.duration_ms)
            .map(|()| e)
            .map_err(Problem::from)
    };
    let id = post.id;
    let save = move |e: FormEvent| {
        e.prevent_default();
        let Ok(body) = request() else {
            return;
        };
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.edit_post(id, &body).await {
                Ok(saved) => onsaved.call(saved.id),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let problem = request().err();
    let changed = problem != Some(Problem::NoChange);

    let title = match kind {
        EditKind::Image => t!("editor-title-image"),
        EditKind::Video => t!("editor-title-video"),
    };
    rsx! {
        div {
            class: "sheet-scrim",
            onclick: move |_| {
                if !busy() {
                    onclose.call(());
                }
            },
            onkeydown: move |e| {
                if e.key() == Key::Escape && !busy() {
                    onclose.call(());
                }
            },
            form {
                class: "card sheet editor",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "editor-title",
                onclick: move |e| e.stop_propagation(),
                onsubmit: save,
                div { class: "sheet-head",
                    h2 { id: "editor-title", class: "card-title", "{title}" }
                    button { class: "icon-btn", r#type: "button", aria_label: t!("share-close"), title: t!("share-close"),
                        disabled: busy(),
                        onclick: move |_| onclose.call(()),
                        Ico { icon: Icon::Close }
                    }
                }
                match kind {
                    EditKind::Image => rsx! {
                        ImageCanvas { src: file_url.clone(), edit, crop, width, height }
                        div { class: "editor-tools", role: "toolbar", aria_label: t!("editor-tools"),
                            button { class: "icon-btn", r#type: "button", title: t!("editor-turn-left"), aria_label: t!("editor-turn-left"),
                                onclick: move |_| {
                                    edit.write().turn(false);
                                    crop.set(CropBox::FULL);
                                },
                                Ico { icon: Icon::RotateLeft }
                            }
                            button { class: "icon-btn", r#type: "button", title: t!("editor-turn-right"), aria_label: t!("editor-turn-right"),
                                onclick: move |_| {
                                    edit.write().turn(true);
                                    crop.set(CropBox::FULL);
                                },
                                Ico { icon: Icon::RotateRight }
                            }
                            button {
                                class: if edit().flip_h { "icon-btn on" } else { "icon-btn" },
                                r#type: "button", title: t!("editor-flip-h"), aria_label: t!("editor-flip-h"),
                                aria_pressed: if edit().flip_h { "true" } else { "false" },
                                onclick: move |_| {
                                    let flipped = !edit().flip_h;
                                    edit.write().flip_h = flipped;
                                    crop.set(crop().mirrored(true));
                                },
                                Ico { icon: Icon::Flip }
                            }
                            button {
                                class: if edit().flip_v { "icon-btn flip-v on" } else { "icon-btn flip-v" },
                                r#type: "button", title: t!("editor-flip-v"), aria_label: t!("editor-flip-v"),
                                aria_pressed: if edit().flip_v { "true" } else { "false" },
                                onclick: move |_| {
                                    let flipped = !edit().flip_v;
                                    edit.write().flip_v = flipped;
                                    crop.set(crop().mirrored(false));
                                },
                                Ico { icon: Icon::Flip }
                            }
                            span { class: "editor-size muted",
                                {
                                    let e = edit();
                                    let (w, h) = e.turned_size(width, height);
                                    match crop().pixels(w, h) {
                                        Some(c) => format!("{} × {}", c.width, c.height),
                                        None => format!("{w} × {h}"),
                                    }
                                }
                            }
                            button { class: "icon-btn", r#type: "button", title: t!("editor-reset"), aria_label: t!("editor-reset"),
                                disabled: !changed,
                                onclick: move |_| {
                                    let bin_original = edit().bin_original;
                                    edit.set(MediaEdit { bin_original, ..MediaEdit::default() });
                                    crop.set(CropBox::FULL);
                                },
                                Ico { icon: Icon::Restore }
                            }
                        }
                        p { class: "muted hint", {t!("editor-crop-hint")} }
                    },
                    EditKind::Video => rsx! {
                        video { id: "edit-video", class: "editor-video", src: "{file_url}", controls: true, playsinline: true, preload: "metadata" }
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
                                let keep = match (parse_seconds(&start()), parse_seconds(&end())) {
                                    (Some(s), Some(e)) if e > s => format_duration(e.min(duration) - s),
                                    _ => None,
                                };
                                match keep {
                                    Some(k) => t!("editor-keeps", length = k, total = format_duration(duration).unwrap_or_default()),
                                    None => t!("editor-bad-time"),
                                }
                            }
                            " "
                            {t!("editor-keyframe-hint")}
                        }
                    },
                }
                label { class: "check",
                    input {
                        r#type: "checkbox",
                        checked: edit().bin_original,
                        onchange: move |e| edit.write().bin_original = e.checked(),
                    }
                    span { {t!("editor-bin-original")} }
                }
                p { class: "muted hint", {t!("editor-copy-hint")} }
                if let Some(message) = error() {
                    ErrorBanner { message }
                } else if let Some(why) = problem.filter(|_| changed) {
                    p { class: "field-error", role: "status", {why.message()} }
                }
                div { class: "row-end",
                    button { class: "btn text", r#type: "button", disabled: busy(), onclick: move |_| onclose.call(()), {t!("cancel")} }
                    button { class: "btn", r#type: "submit", disabled: busy() || problem.is_some(),
                        Ico { icon: Icon::Check }
                        if busy() { {t!("editor-saving")} } else { {t!("editor-save")} }
                    }
                }
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
                TimeButton { onnow }
            }
        }
    }
}

#[component]
fn TimeButton(onnow: EventHandler<i64>) -> Element {
    rsx! {
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

/// The image as it will be saved, with the crop box over it.
#[component]
fn ImageCanvas(
    src: String,
    edit: Signal<MediaEdit>,
    crop: Signal<CropBox>,
    width: u32,
    height: u32,
) -> Element {
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let mut rect = use_signal(|| None::<PixelsRect>);
    let mut drag = use_signal(|| None::<Drag>);
    let measure = move || async move {
        let handle = mounted.peek().clone();
        if let Some(m) = handle
            && let Ok(r) = m.get_client_rect().await
        {
            rect.set(Some(r));
        }
    };
    let (frame, image) = preview_styles(&edit(), width, height);
    let mut begin = move |handle: Handle, e: PointerEvent| {
        e.stop_propagation();
        e.prevent_default();
        drag.set(Some(Drag {
            handle,
            from: e.client_coordinates(),
            start: crop(),
        }));
        spawn(async move { measure().await });
    };
    // Every pointer leaving the frame ends up here; re-render only on a change.
    let mut drop_handle = move || {
        if drag.peek().is_some() {
            drag.set(None);
        }
    };
    rsx! {
        div { class: "editor-stage",
            div {
                class: if drag().is_some() { "editor-frame dragging" } else { "editor-frame" },
                style: "{frame}",
                onmounted: move |e| {
                    mounted.set(Some(e.data()));
                    spawn(async move { measure().await });
                },
                onresize: move |_| {
                    spawn(async move { measure().await });
                },
                onpointermove: move |e| {
                    let (Some(d), Some(r)) = (drag(), rect()) else {
                        return;
                    };
                    if r.width() < 1.0 || r.height() < 1.0 {
                        return;
                    }
                    let p = e.client_coordinates();
                    let dx = (p.x - d.from.x) / r.width();
                    let dy = (p.y - d.from.y) / r.height();
                    crop.set(d.start.dragged(d.handle, dx, dy));
                },
                onpointerup: move |_| drop_handle(),
                onpointercancel: move |_| drop_handle(),
                onpointerleave: move |_| drop_handle(),
                img { src: "{src}", alt: "", draggable: "false", style: "{image}" }
                div {
                    class: "crop-box",
                    style: crop().style(),
                    onpointerdown: move |e| begin(Handle::Move, e),
                    for (handle, name) in Handle::CORNERS {
                        span {
                            key: "{name}",
                            class: "crop-handle {name}",
                            onpointerdown: move |e| begin(handle, e),
                        }
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

    #[test]
    fn limits_never_panic() {
        assert_eq!(within(5.0, 0.0, 1.0), 1.0);
        assert_eq!(within(-5.0, 0.0, 1.0), 0.0);
        // `f64::clamp` panics on both of these.
        assert_eq!(within(0.5, 11.0, 10.0), 11.0);
        assert_eq!(within(f64::NAN, 0.0, 1.0), 0.0);
        let odd = CropBox {
            x: f64::NAN,
            y: 2.0,
            w: -1.0,
            h: f64::INFINITY,
        };
        let _ = odd.dragged(Handle::TopLeft, f64::NAN, 1.0).pixels(10, 10);
        let _ = odd.pixels(0, 0);
    }

    #[test]
    fn drags_the_crop() {
        let full = CropBox::FULL;
        let b = full.dragged(Handle::TopLeft, 0.25, 0.5);
        assert!(close(
            b,
            CropBox {
                x: 0.25,
                y: 0.5,
                w: 0.75,
                h: 0.5
            }
        ));
        // Dragging past the edge or the opposite corner stops there.
        let b = full.dragged(Handle::BottomRight, 0.5, -2.0);
        assert!(close(
            b,
            CropBox {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: MIN_CROP
            }
        ));
        let b = full.dragged(Handle::TopRight, 3.0, 0.0);
        assert!(close(b, full));
        let small = CropBox {
            x: 0.2,
            y: 0.2,
            w: 0.5,
            h: 0.5,
        };
        let b = small.dragged(Handle::Move, 0.6, -0.6);
        assert!(close(
            b,
            CropBox {
                x: 0.5,
                y: 0.0,
                ..small
            }
        ));
        let b = small.dragged(Handle::BottomLeft, -0.1, 0.1);
        assert!(close(
            b,
            CropBox {
                x: 0.1,
                y: 0.2,
                w: 0.6,
                h: 0.6
            }
        ));
        // Mirroring keeps the same part of the picture.
        let b = small.mirrored(true);
        assert!(close(b, CropBox { x: 0.3, ..small }));
        assert!(close(b.mirrored(true), small));
    }

    #[test]
    fn crops_in_pixels() {
        assert_eq!(CropBox::FULL.pixels(40, 30), None);
        let b = CropBox {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        assert_eq!(
            b.pixels(40, 30),
            Some(Crop {
                x: 10,
                y: 0,
                width: 20,
                height: 30
            })
        );
        // Always at least one pixel, and inside the image.
        let tiny = CropBox {
            x: 0.999,
            y: 0.999,
            w: 0.0001,
            h: 0.0001,
        };
        let c = tiny.pixels(10, 10).unwrap();
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
    fn previews_turns() {
        let mut e = MediaEdit::default();
        let (frame, image) = preview_styles(&e, 400, 200);
        assert!(frame.starts_with("aspect-ratio: 400 / 200"), "{frame}");
        assert!(
            image.contains("width: 100.000%; height: 100.000%"),
            "{image}"
        );
        e.turn(true);
        e.flip_h = true;
        let (frame, image) = preview_styles(&e, 400, 200);
        assert!(frame.starts_with("aspect-ratio: 200 / 400"), "{frame}");
        assert!(
            image.contains("width: 200.000%; height: 50.000%"),
            "{image}"
        );
        assert!(image.ends_with("scale(-1, 1) rotate(90deg)"), "{image}");
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
}
