//! Zoom and pan state of the media viewer. Pure maths, so it is unit tested.

use dioxus::html::geometry::WheelDelta;

pub const MIN_SCALE: f64 = 1.0;
pub const MAX_SCALE: f64 = 8.0;
/// Scale a double click or tap zooms to.
pub const TAP_SCALE: f64 = 2.5;
/// Factor of one zoom-in or zoom-out step (buttons and keys).
pub const STEP: f64 = 1.5;
/// Factor of one notch of a mouse wheel.
const WHEEL_NOTCH: f64 = 1.1;
/// A pixel delta this big is a whole notch, whatever its size. On Linux,
/// Chromium sends 120 for one and WebKitGTK 80, at any display scale.
const NOTCH_MIN_PIXELS: f64 = 40.0;
/// Smaller pixel deltas (touchpads, smooth wheels) add up to a notch at this.
const PIXELS_PER_NOTCH: f64 = 100.0;
/// Firefox and its forks send lines: 3 for one notch by default, 6 in Zen.
const LINES_PER_NOTCH: f64 = 3.0;

/// The zoom factor of one wheel event, the same for a notch of the wheel on
/// every browser, platform and display scale. An event counts as one notch at
/// most, so a long swipe or a big delta can't jump the zoom.
pub fn wheel_factor(delta: WheelDelta) -> f64 {
    let notches = match delta {
        WheelDelta::Pixels(v) if v.y.abs() >= NOTCH_MIN_PIXELS => v.y.signum(),
        WheelDelta::Pixels(v) => v.y / PIXELS_PER_NOTCH,
        WheelDelta::Lines(v) => (v.y / LINES_PER_NOTCH).clamp(-1.0, 1.0),
        WheelDelta::Pages(v) => v.y.clamp(-1.0, 1.0),
    };
    // Scrolling down (positive) zooms out.
    WHEEL_NOTCH.powf(-notches)
}

/// The media's scale and offset, in CSS pixels from the stage centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom {
    pub scale: f64,
    pub x: f64,
    pub y: f64,
}

impl Default for Zoom {
    fn default() -> Self {
        Zoom {
            scale: MIN_SCALE,
            x: 0.0,
            y: 0.0,
        }
    }
}

impl Zoom {
    pub fn is_zoomed(&self) -> bool {
        self.scale > MIN_SCALE + 0.001
    }

    /// Scales by `factor`, keeping the point `at` (relative to the stage
    /// centre) under the cursor. `bounds` is the stage size.
    pub fn zoom_at(self, factor: f64, at: (f64, f64), bounds: (f64, f64)) -> Self {
        if !factor.is_finite() || factor <= 0.0 || !at.0.is_finite() || !at.1.is_finite() {
            return self;
        }
        let scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let k = scale / self.scale;
        Zoom {
            scale,
            x: at.0 - (at.0 - self.x) * k,
            y: at.1 - (at.1 - self.y) * k,
        }
        .clamped(bounds)
    }

    /// Zoomed in: back to fit. At fit: zoom in on `at`.
    pub fn toggle_at(self, at: (f64, f64), bounds: (f64, f64)) -> Self {
        if self.is_zoomed() {
            Zoom::default()
        } else {
            self.zoom_at(TAP_SCALE / self.scale, at, bounds)
        }
    }

    pub fn pan(self, dx: f64, dy: f64, bounds: (f64, f64)) -> Self {
        if !dx.is_finite() || !dy.is_finite() {
            return self;
        }
        Zoom {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
        .clamped(bounds)
    }

    /// Keeps the stage covered: the offset is at most half the overflow.
    fn clamped(self, (w, h): (f64, f64)) -> Self {
        if !self.is_zoomed() {
            return Zoom::default();
        }
        let over = self.scale - MIN_SCALE;
        // `max` also turns a NaN size into 0.
        let mx = w.max(0.0) * over / 2.0;
        let my = h.max(0.0) * over / 2.0;
        Zoom {
            x: self.x.clamp(-mx, mx),
            y: self.y.clamp(-my, my),
            ..self
        }
    }

    pub fn css(&self) -> String {
        format!(
            "transform: translate({:.1}px, {:.1}px) scale({:.3})",
            self.x, self.y, self.scale
        )
    }

    pub fn percent(&self) -> u32 {
        // Scale is clamped to 1..=8, so this always fits.
        (self.scale * 100.0).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: (f64, f64) = (800.0, 600.0);

    #[test]
    fn zooms_around_the_cursor() {
        let z = Zoom::default().zoom_at(2.0, (100.0, 50.0), B);
        assert_eq!(z.scale, 2.0);
        // The point under the cursor stays put: at = t + (at - t0) * k.
        assert_eq!((z.x, z.y), (-100.0, -50.0));
        let centre = Zoom::default().zoom_at(2.0, (0.0, 0.0), B);
        assert_eq!((centre.x, centre.y), (0.0, 0.0));
    }

    #[test]
    fn clamps_scale_and_offset() {
        let z = Zoom::default().zoom_at(100.0, (0.0, 0.0), B);
        assert_eq!(z.scale, MAX_SCALE);
        let out = Zoom::default().zoom_at(0.1, (10.0, 10.0), B);
        assert_eq!(out, Zoom::default());
        let z = Zoom::default()
            .zoom_at(2.0, (0.0, 0.0), B)
            .pan(10_000.0, -10_000.0, B);
        assert_eq!((z.x, z.y), (400.0, -300.0));
        // Not zoomed: panning does nothing.
        assert_eq!(Zoom::default().pan(50.0, 50.0, B), Zoom::default());
    }

    #[test]
    fn toggles() {
        let z = Zoom::default().toggle_at((0.0, 0.0), B);
        assert_eq!(z.scale, TAP_SCALE);
        assert_eq!(z.toggle_at((5.0, 5.0), B), Zoom::default());
    }

    #[test]
    fn ignores_hostile_numbers() {
        let z = Zoom::default().zoom_at(2.0, (0.0, 0.0), B);
        assert_eq!(z.zoom_at(f64::NAN, (0.0, 0.0), B), z);
        assert_eq!(z.zoom_at(-1.0, (0.0, 0.0), B), z);
        assert_eq!(z.zoom_at(2.0, (f64::INFINITY, 0.0), B), z);
        assert_eq!(z.pan(f64::NAN, 1.0, B), z);
        let nan_bounds = z.pan(5.0, 5.0, (f64::NAN, f64::NAN));
        assert_eq!((nan_bounds.x, nan_bounds.y), (0.0, 0.0));
        assert_eq!(z.percent(), 200);
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_wheel_notch_is_the_same_everywhere() {
        // One notch, in the units and sizes engines send.
        for notch in [
            WheelDelta::pixels(0.0, 120.0, 0.0),
            WheelDelta::pixels(0.0, 100.0, 0.0),
            WheelDelta::pixels(0.0, 150.0, 0.0),
            WheelDelta::pixels(0.0, 80.0, 0.0),
            WheelDelta::lines(0.0, 3.0, 0.0),
            WheelDelta::lines(0.0, 6.0, 0.0),
            WheelDelta::pages(0.0, 1.0, 0.0),
        ] {
            assert!(close(wheel_factor(notch), 1.0 / WHEEL_NOTCH));
        }
        for notch in [
            WheelDelta::pixels(0.0, -120.0, 0.0),
            WheelDelta::pixels(0.0, -80.0, 0.0),
            WheelDelta::lines(0.0, -6.0, 0.0),
        ] {
            assert!(close(wheel_factor(notch), WHEEL_NOTCH));
        }
    }

    #[test]
    fn small_wheel_deltas_add_up_to_a_notch() {
        // A touchpad or smooth wheel: ten events of 10 px make one notch.
        let event = wheel_factor(WheelDelta::pixels(0.0, 10.0, 0.0));
        assert!(close(event.powi(10), 1.0 / WHEEL_NOTCH));
        let event = wheel_factor(WheelDelta::lines(0.0, 0.75, 0.0));
        assert!(close(event.powi(4), 1.0 / WHEEL_NOTCH));
    }

    #[test]
    fn a_wheel_event_never_jumps() {
        assert!(close(
            wheel_factor(WheelDelta::pixels(0.0, -10_000.0, 0.0)),
            WHEEL_NOTCH
        ));
        assert!(close(
            wheel_factor(WheelDelta::lines(0.0, -500.0, 0.0)),
            WHEEL_NOTCH
        ));
        assert!(close(
            wheel_factor(WheelDelta::pages(0.0, -9.0, 0.0)),
            WHEEL_NOTCH
        ));
    }

    #[test]
    fn a_wheel_event_without_vertical_travel_does_nothing() {
        assert_eq!(wheel_factor(WheelDelta::pixels(50.0, 0.0, 0.0)), 1.0);
        assert_eq!(wheel_factor(WheelDelta::lines(3.0, 0.0, 0.0)), 1.0);
    }

    #[test]
    fn hostile_wheel_deltas_leave_the_zoom_alone() {
        let z = Zoom::default().zoom_at(2.0, (0.0, 0.0), B);
        for delta in [
            WheelDelta::pixels(0.0, f64::NAN, 0.0),
            WheelDelta::lines(0.0, f64::NAN, 0.0),
            WheelDelta::pages(0.0, f64::NAN, 0.0),
        ] {
            assert_eq!(z.zoom_at(wheel_factor(delta), (0.0, 0.0), B), z);
        }
        let huge = wheel_factor(WheelDelta::pixels(0.0, f64::INFINITY, 0.0));
        assert!(close(huge, 1.0 / WHEEL_NOTCH));
    }
}
