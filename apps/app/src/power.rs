//! Battery saver: less downloading, decoding and animating while it's on.
//! Automatic (the default) follows the device: on when the battery is low
//! and not charging, or when the device asks to save data.
//!
//! While it's on, videos don't start by themselves, the viewer doesn't
//! fetch the next post ahead or swap in originals for dense screens, the
//! gallery doesn't preview videos on hover, and animations are cut short.
//! Videos always pause while the app is hidden.

use dioxus::prelude::*;

use crate::store::{PREFS, Saver};

/// Set by the device watcher: the battery is low or data is to be saved.
static DEVICE_LOW: GlobalSignal<bool> = Signal::global(|| false);

/// Reports the device's state, and pauses every video when the app is
/// hidden. Sends `true` while the battery is at 20% or less and not
/// charging, or Data Saver is on. Either API may be missing (the battery
/// one is Chromium-only); then that part never counts as low.
const WATCH: &str = r#"
const conn = navigator.connection;
let battery = null;
const low = () => !!(conn && conn.saveData) || !!(battery && !battery.charging && battery.level <= 0.2);
const update = () => dioxus.send(low());
try {
  if (navigator.getBattery) {
    battery = await navigator.getBattery();
    battery.addEventListener("levelchange", update);
    battery.addEventListener("chargingchange", update);
  }
} catch (_) {}
if (conn && conn.addEventListener) conn.addEventListener("change", update);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) document.querySelectorAll("video").forEach((v) => v.pause());
});
update();
"#;

/// Whether the saver is on, for a setting and the device's state.
pub fn saving(setting: Saver, device_low: bool) -> bool {
    match setting {
        Saver::Always => true,
        Saver::Never => false,
        Saver::Auto => device_low,
    }
}

/// Whether the saver is on now. Reading it subscribes to changes.
pub fn active() -> bool {
    saving(PREFS.read().battery_saver, DEVICE_LOW())
}

/// Whether the device is low right now (for Settings).
pub fn device_low() -> bool {
    DEVICE_LOW()
}

/// Starts watching the device. Call once, from the root component.
pub fn watch() {
    use_future(|| async {
        let mut events = document::eval(WATCH);
        while let Ok(low) = events.recv::<bool>().await {
            if *DEVICE_LOW.peek() != low {
                *DEVICE_LOW.write() = low;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_device_only_when_automatic() {
        assert!(saving(Saver::Always, false));
        assert!(!saving(Saver::Never, true));
        assert!(saving(Saver::Auto, true));
        assert!(!saving(Saver::Auto, false));
    }
}
