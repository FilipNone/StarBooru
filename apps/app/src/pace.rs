//! Effects the device can afford.
//!
//! The frosted glass of the bars, the entrance of new thumbnails and the glow
//! behind a picture cost little on a desktop and too much on a small board or
//! an old phone, where they make scrolling stutter. Nothing a page can read
//! says which kind of device it is on, so the app times the frames of real
//! scrolling instead. A device that keeps losing frames gets the reduced
//! effects from then on, and the setting in Settings › Appearance overrides
//! the measurement either way.
//!
//! While they are reduced, the root element carries `data-effects="lite"` and
//! `main.css` does the rest.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::store::{Effects, PREFS};

/// One stretch of scrolling as the page timed it: how many frames followed its
/// first, how long they took in milliseconds, and the time between two frames
/// once the page had come to rest, which is what the screen can show.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Burst(pub f64, pub f64, pub f64);

/// A shorter stretch than this many frames says too little.
const MIN_FRAMES: f64 = 20.0;
/// The share of frames a stretch can lose and still count as smooth.
const ROUGH: f64 = 0.25;
/// Rough stretches, less the smooth ones in between, before the effects go.
/// One bad stretch can be a page of results arriving or another program.
const STRIKES: u8 = 3;
/// A gap shorter than this is two callbacks of one frame, as no screen
/// refreshes faster than 250 times a second.
const SHORTEST_FRAME_MS: f64 = 4.0;

/// Whether a stretch lost too many of the frames its screen could have shown.
/// `None` if it was too short to tell, or the numbers make no sense.
pub fn rough(frames: f64, ms: f64, frame_ms: f64) -> Option<bool> {
    let sane = frames.is_finite() && ms.is_finite() && frame_ms.is_finite();
    if !sane || frames < 0.0 || frame_ms < SHORTEST_FRAME_MS {
        return None;
    }
    let possible = ms / frame_ms;
    (possible >= MIN_FRAMES).then(|| 1.0 - frames / possible > ROUGH)
}

/// The stretches seen so far.
#[derive(Debug, Default)]
pub struct Pace {
    strikes: u8,
}

impl Pace {
    /// Takes one stretch. True once the device has shown it can't keep up.
    pub fn take(&mut self, Burst(frames, ms, frame_ms): Burst) -> bool {
        match rough(frames, ms, frame_ms) {
            Some(true) => self.strikes = self.strikes.saturating_add(1),
            Some(false) => self.strikes = self.strikes.saturating_sub(1),
            None => {}
        }
        self.strikes >= STRIKES
    }
}

/// Whether the effects are reduced, for a setting and what was measured.
pub fn reduced(setting: Effects, measured_slow: bool) -> bool {
    match setting {
        Effects::Lite => true,
        Effects::Full => false,
        Effects::Auto => measured_slow,
    }
}

/// Whether the effects are reduced now. Reading it subscribes to changes.
pub fn lite() -> bool {
    let prefs = PREFS.read();
    reduced(prefs.effects, prefs.effects_reduced)
}

/// Times each stretch of scrolling while `window.__sbPace.on` is set: from the
/// first scroll event until 150 ms pass without one. Any scroller counts, as
/// the listener takes the events on their way down. Then it times eight frames
/// of the page at rest. Those say how fast the screen is right now, which a
/// number kept from earlier wouldn't after a change of monitor or of power
/// profile. Scrolling again during those frames carries the stretch on.
/// Sends a `Burst`.
const WATCH: &str = r#"
window.__sbPace ??= { on: false };
const REST = 8;
let running = false, until = 0, first = 0, end = 0, last = 0, count = 0, rest = 0, quickest = Infinity;
const frame = (now) => {
  if (now < until) {
    // Scrolling, or scrolling again: the frames at rest so far were part of it.
    if (!count) first = now;
    count += 1 + rest;
    rest = 0;
    quickest = Infinity;
    end = last = now;
    return requestAnimationFrame(frame);
  }
  rest += 1;
  // The first of these gaps still ends the scrolling.
  if (rest > 1) quickest = Math.min(quickest, now - last);
  last = now;
  if (rest <= REST) return requestAnimationFrame(frame);
  running = false;
  if (count > 1 && Number.isFinite(quickest)) dioxus.send([count - 1, end - first, quickest]);
};
addEventListener("scroll", () => {
  until = performance.now() + 150;
  if (running || !window.__sbPace.on) return;
  running = true;
  count = rest = 0;
  quickest = Infinity;
  requestAnimationFrame(frame);
}, { passive: true, capture: true });
"#;

/// Starts timing. Call once, from the root component.
pub fn watch() {
    // Only Automatic measures, and only until it has its answer.
    let measuring = use_memo(|| {
        let prefs = PREFS.read();
        prefs.effects == Effects::Auto && !prefs.effects_reduced
    });
    use_effect(move || {
        let on = measuring();
        document::eval(&format!("(window.__sbPace ??= {{}}).on = {on};"));
    });
    use_future(move || async move {
        let mut pace = Pace::default();
        let mut bursts = document::eval(WATCH);
        while let Ok(burst) = bursts.recv::<Burst>().await {
            if !*measuring.peek() {
                pace = Pace::default();
            } else if pace.take(burst) {
                PREFS.write().effects_reduced = true;
                pace = Pace::default();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second of scrolling on a 60 Hz screen that drew `frames` frames.
    fn second(frames: f64) -> Burst {
        Burst(frames, 1000.0, 1000.0 / 60.0)
    }

    #[test]
    fn a_stretch_is_rough_past_a_quarter_of_its_frames_lost() {
        assert_eq!(rough(60.0, 1000.0, 16.7), Some(false));
        assert_eq!(rough(50.0, 1000.0, 16.7), Some(false));
        assert_eq!(rough(40.0, 1000.0, 16.7), Some(true));
        // A 30 Hz screen, or a webview held to 30 by the power profile, at
        // full speed.
        assert_eq!(rough(30.0, 1000.0, 33.3), Some(false));
    }

    #[test]
    fn a_short_stretch_says_nothing() {
        assert_eq!(rough(2.0, 200.0, 16.7), None);
        assert_eq!(rough(0.0, 0.0, 16.7), None);
    }

    #[test]
    fn hostile_numbers_say_nothing() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(rough(bad, 1000.0, 16.7), None);
            assert_eq!(rough(30.0, bad, 16.7), None);
            assert_eq!(rough(30.0, 1000.0, bad), None);
        }
        assert_eq!(rough(-5.0, 1000.0, 16.7), None);
        assert_eq!(rough(30.0, 1000.0, 0.0), None);
        assert_eq!(rough(30.0, 1000.0, -16.7), None);
        let mut pace = Pace::default();
        for _ in 0..50 {
            assert!(!pace.take(Burst(f64::NAN, f64::INFINITY, -1.0)));
            assert!(!pace.take(Burst(1.0, 1e300, 5.0e-300)));
        }
    }

    #[test]
    fn three_rough_stretches_reduce_the_effects() {
        let mut pace = Pace::default();
        assert!(!pace.take(second(35.0)));
        assert!(!pace.take(second(35.0)));
        assert!(pace.take(second(35.0)));
    }

    #[test]
    fn a_smooth_device_keeps_its_effects() {
        let mut pace = Pace::default();
        for _ in 0..200 {
            assert!(!pace.take(second(59.0)));
        }
        // A hiccup now and then is forgiven by the smooth stretches after it.
        for _ in 0..50 {
            assert!(!pace.take(second(30.0)));
            assert!(!pace.take(second(60.0)));
        }
    }

    #[test]
    fn a_steady_half_speed_is_rough_whatever_its_own_frames_say() {
        // Every frame took 33 ms on a 60 Hz screen. Judged by its own quickest
        // frame this would pass for a 30 Hz screen at full speed.
        let mut pace = Pace::default();
        let slow = Burst(30.0, 1000.0, 1000.0 / 60.0);
        assert!(!pace.take(slow));
        assert!(!pace.take(slow));
        assert!(pace.take(slow));
    }

    #[test]
    fn a_slower_screen_is_no_slower_device() {
        // 120 Hz, then the window moves to a 60 Hz monitor.
        let mut pace = Pace::default();
        assert!(!pace.take(Burst(118.0, 1000.0, 1000.0 / 120.0)));
        for _ in 0..20 {
            assert!(!pace.take(second(59.0)));
        }
    }

    #[test]
    fn reduced_effects_leave_out_what_costs_most() {
        let css = include_str!("../assets/main.css");
        let (_, glass) = css
            .split_once(r#".app[data-effects="lite"] .glass {"#)
            .unwrap();
        let (glass, _) = glass.split_once('}').unwrap();
        assert!(glass.contains("backdrop-filter: none"));
        // Opaque, or the page would show through unblurred.
        assert!(glass.contains("/ 1)"));
        assert!(
            css.contains(r#".thumb.still, .app[data-effects="lite"] .thumb { animation: none; }"#)
        );
        assert!(css.contains(r#".app[data-effects="lite"] .post-glow { display: none; }"#));
    }

    #[test]
    fn the_page_only_times_frames_when_asked() {
        assert!(WATCH.contains("if (running || !window.__sbPace.on) return;"));
        // Frames of the page at rest say how fast the screen is.
        assert!(WATCH.contains("if (rest <= REST) return requestAnimationFrame(frame);"));
    }

    #[test]
    fn the_setting_overrides_the_measurement() {
        assert!(reduced(Effects::Lite, false));
        assert!(!reduced(Effects::Full, true));
        assert!(reduced(Effects::Auto, true));
        assert!(!reduced(Effects::Auto, false));
    }
}
