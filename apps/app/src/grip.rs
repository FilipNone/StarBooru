//! The handle that makes a text box taller. Browsers draw their own in the
//! corner, each differently and none of them in the app's colours, so CSS
//! turns it off (`resize: none`) and a [`Grip`] stands in for it. One script
//! on the window does the dragging for every grip, so a move costs no trip
//! to Rust in the desktop and Android apps.

use dioxus::prelude::*;

/// Drags a `.grip` up and down to set the height of the text box beside it.
/// The height never goes under the box's `min-height`, nor under two lines.
pub const SCRIPT: &str = r#"
(() => {
  if (window.__sbGrip) return;
  window.__sbGrip = true;
  addEventListener("pointerdown", (e) => {
    const grip = e.target instanceof Element ? e.target.closest(".grip") : null;
    if (!grip || e.button !== 0) return;
    const box = grip.parentElement?.querySelector("textarea");
    if (!box) return;
    e.preventDefault();
    const from = e.clientY;
    const start = box.getBoundingClientRect().height;
    const least = Math.max(parseFloat(getComputedStyle(box).minHeight) || 0, 64);
    let y = from;
    let frame = 0;
    const size = () => {
      frame = 0;
      // Lets CSS tell a box sized by hand from one at its own height.
      if (!("sized" in box.dataset)) box.dataset.sized = "";
      box.style.height = `${Math.max(least, start + y - from)}px`;
    };
    const move = (m) => {
      if (m.pointerId !== e.pointerId) return;
      y = m.clientY;
      frame ||= requestAnimationFrame(size);
    };
    const stop = (m) => {
      if (m.pointerId !== e.pointerId) return;
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", stop);
      grip.removeEventListener("pointercancel", stop);
      grip.classList.remove("held");
    };
    grip.setPointerCapture(e.pointerId);
    grip.classList.add("held");
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", stop);
    grip.addEventListener("pointercancel", stop);
  });
})();
"#;

/// Puts the script in place. Call once, from the root component.
pub fn install() {
    use_hook(|| {
        document::eval(SCRIPT);
    });
}

/// The handle itself. It goes right after the `textarea`, inside a
/// `.text-box` that holds the two.
#[component]
pub fn Grip() -> Element {
    rsx! {
        span { class: "grip", aria_hidden: "true" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = include_str!("../assets/main.css");

    #[test]
    fn the_browsers_handle_is_off_everywhere() {
        assert!(CSS.contains("textarea { resize: none; }"));
        assert!(!CSS.contains("resize: vertical"));
        assert!(!CSS.contains("resize: both"));
    }

    #[test]
    fn every_text_box_has_a_grip() {
        let mut all = 0;
        for (name, page) in [
            ("upload", include_str!("pages/upload.rs")),
            ("post", include_str!("pages/post.rs")),
            ("tag box", include_str!("pages/tag_box.rs")),
        ] {
            let boxes = page.lines().filter(|l| l.trim() == "textarea {").count();
            all += boxes;
            assert_eq!(page.matches("Grip {}").count(), boxes, "{name}");
            assert_eq!(
                page.matches(r#"class: "text-box""#).count(),
                boxes,
                "{name}"
            );
        }
        assert_eq!(all, 2);
    }

    #[test]
    fn the_grip_moves_in_frames_and_lets_go() {
        assert!(SCRIPT.contains("if (window.__sbGrip) return;"));
        assert!(SCRIPT.contains("requestAnimationFrame(size)"));
        assert!(SCRIPT.contains("setPointerCapture"));
        assert!(SCRIPT.contains(r#"removeEventListener("pointercancel", stop)"#));
        assert!(SCRIPT.contains("Math.max(least,"));
        assert!(SCRIPT.contains(r#"box.dataset.sized = "";"#));
    }

    #[test]
    fn the_grip_is_drawn_in_the_app_colours() {
        let grip = CSS
            .split_once(".grip {")
            .map(|(_, rest)| rest)
            .unwrap_or("");
        assert!(grip.contains("touch-action: none"));
        assert!(CSS.contains(".grip::before"));
        assert!(CSS.contains(".grip:is(:hover, .held)"));
    }
}
