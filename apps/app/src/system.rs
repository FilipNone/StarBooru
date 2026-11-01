//! What the device says about itself: the app's own folders on Android, and
//! the Material You accent that Android 12 and later take from the
//! wallpaper.

#![cfg_attr(
    not(target_os = "android"),
    allow(dead_code, reason = "only Android has a wallpaper colour to read")
)]

use dioxus::prelude::*;

use starbooru_core::color::Rgb;

use crate::store::Prefs;

/// The wallpaper's accent hue, when the device has one. Read at start and
/// again whenever the app comes back to the front, since the wallpaper may
/// have changed meanwhile.
pub static SYSTEM_HUE: GlobalSignal<Option<u16>> = Signal::global(read_hue);

/// Below this OKLCH chroma a colour reads as grey and its hue is noise.
const MIN_CHROMA: f64 = 0.02;

fn read_hue() -> Option<u16> {
    #[cfg(target_os = "android")]
    return android::accent().and_then(hue_of);
    #[cfg(not(target_os = "android"))]
    None
}

/// The hue the palette is built from: the wallpaper's when the user follows
/// it and the device has one, otherwise the one they picked.
pub fn seed_hue(prefs: &Prefs) -> u16 {
    pick_hue(prefs, SYSTEM_HUE())
}

fn pick_hue(prefs: &Prefs, system: Option<u16>) -> u16 {
    match system {
        Some(hue) if prefs.system_accent => hue,
        _ => prefs.accent_hue,
    }
}

/// Re-reads the wallpaper's accent when the app becomes visible again.
pub fn watch() {
    #[cfg(target_os = "android")]
    use_future(|| async {
        let mut eval = document::eval(
            r#"document.addEventListener("visibilitychange", () => {
                if (document.visibilityState === "visible") dioxus.send(true);
            });"#,
        );
        while eval.recv::<bool>().await.is_ok() {
            let hue = read_hue();
            if *SYSTEM_HUE.peek() != hue {
                *SYSTEM_HUE.write() = hue;
            }
        }
    });
}

/// OKLCH hue in whole degrees (0–359) of an `0xAARRGGBB` colour, or `None`
/// when the colour is too grey to have one.
pub fn hue_of(argb: u32) -> Option<u16> {
    let [_, r, g, b] = argb.to_be_bytes();
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(r), lin(g), lin(b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    if a.hypot(b) < MIN_CHROMA {
        return None;
    }
    let degrees = b.atan2(a).to_degrees().rem_euclid(360.0).round();
    // `degrees` is in 0..=360, so it fits; 360 wraps to 0.
    Some((degrees as u16) % 360)
}

/// The colour an accent preset of this hue is drawn in, as `.accent` in
/// `main.css`.
pub fn swatch_rgb(hue: u16) -> Rgb {
    srgb(0.6, 0.16, f64::from(hue)).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// OKLCH to gamma-encoded sRGB, unclamped, 1.0 being full.
fn srgb(l: f64, c: f64, h: f64) -> [f64; 3] {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let lms = [
        (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3),
        (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3),
        (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3),
    ];
    [
        4.076_741_662_1 * lms[0] - 3.307_711_591_3 * lms[1] + 0.230_969_929_2 * lms[2],
        -1.268_438_004_6 * lms[0] + 2.609_757_401_1 * lms[1] - 0.341_319_396_5 * lms[2],
        -0.004_196_086_3 * lms[0] - 0.703_418_614_7 * lms[1] + 1.707_614_701 * lms[2],
    ]
    .map(|c| {
        if c <= 0.003_130_8 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// The app's private folders, from `Context`. `directories` can't find them:
/// Android apps have no home directory.
#[cfg(target_os = "android")]
pub mod android {
    use std::path::PathBuf;

    use jni::JNIEnv;
    use jni::objects::{JObject, JString, JValue};

    /// Runs `f` with the activity inside its own local frame, so the
    /// references it makes are freed on this permanently attached thread,
    /// and clears any Java exception it leaves behind.
    fn call<T>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T>) -> Option<T> {
        robius_android_env::with_activity(|env, activity| {
            let out = env.with_local_frame(16, |env| f(env, activity));
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            out.ok()
        })
        .ok()
        .flatten()
    }

    fn dir(getter: &str) -> Option<PathBuf> {
        call(|env, ctx| {
            let file = env.call_method(ctx, getter, "()Ljava/io/File;", &[])?.l()?;
            let path = env
                .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                .l()?;
            let path: String = env.get_string(&JString::from(path))?.into();
            Ok(PathBuf::from(path))
        })
    }

    /// Private storage that lasts: settings and the session.
    pub fn files_dir() -> Option<PathBuf> {
        dir("getFilesDir")
    }

    /// Private storage Android may clear when space runs low.
    pub fn cache_dir() -> Option<PathBuf> {
        dir("getCacheDir")
    }

    /// The wallpaper's main accent (`system_accent1_500`) as `0xAARRGGBB`,
    /// on Android 12 (API 31) and later.
    pub fn accent() -> Option<u32> {
        call(|env, ctx| {
            let sdk = env
                .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
                .i()?;
            if sdk < 31 {
                return Ok(None);
            }
            let id = env
                .get_static_field("android/R$color", "system_accent1_500", "I")?
                .i()?;
            let argb = env
                .call_method(ctx, "getColor", "(I)I", &[JValue::Int(id)])?
                .i()?;
            Ok(Some(u32::from_ne_bytes(argb.to_ne_bytes())))
        })
        .flatten()
    }
}

/// Whether Dioxus should turn off WebKit's DMA-BUF renderer and run the
/// Linux desktop app through XWayland, as it does by default on Wayland.
/// That copies every frame through memory, which made the app crawl on a
/// large scaled screen, so it's kept for NVIDIA's driver only, whose
/// explicit sync breaks the renderer. Otherwise GTK picks Wayland when
/// there is one (the window's title bar comes from the patched tao in
/// `vendor/tao`). `STARBOORU_DMABUF=0` or `=1` overrides the guess.
#[cfg(feature = "desktop")]
pub fn dmabuf_off_here() -> bool {
    let forced = std::env::var("STARBOORU_DMABUF").ok();
    dmabuf_off(
        forced.as_deref(),
        std::path::Path::new("/sys/module/nvidia").exists(),
    )
}

#[cfg(feature = "desktop")]
fn dmabuf_off(forced: Option<&str>, nvidia: bool) -> bool {
    match forced.map(str::trim) {
        Some("0") => true,
        Some("1") => false,
        _ => nvidia,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "desktop")]
    #[test]
    fn dmabuf_off_only_for_nvidia() {
        // Regression: every Wayland desktop ran the slow copying renderer.
        assert!(!dmabuf_off(None, false));
        assert!(dmabuf_off(None, true));
        assert!(dmabuf_off(Some("0"), false));
        assert!(!dmabuf_off(Some(" 1 "), true));
        assert!(dmabuf_off(Some("yes please"), true));
    }

    #[test]
    fn finds_the_hue_of_a_colour() {
        let near = |argb, want: u16| {
            let got = hue_of(argb).unwrap();
            assert!(got.abs_diff(want) <= 1, "{argb:#x}: {got} vs {want}");
        };
        near(0xFFFF_0000, 29);
        near(0xFF00_FF00, 142);
        near(0xFF00_00FF, 264);
        // Material's baseline purple, the default seed on Android 12.
        near(0xFF67_50A4, 294);
        // Alpha doesn't matter.
        near(0x0000_00FF, 264);
    }

    #[test]
    fn greys_have_no_hue() {
        for grey in [0xFF00_0000, 0xFF80_8080, 0xFFFF_FFFF, 0xFF7F_8081] {
            assert_eq!(hue_of(grey), None, "{grey:#x}");
        }
    }

    #[test]
    fn the_hue_stays_below_360() {
        // Every hue of a saturated colour wheel lands in 0..360.
        for i in 0..=255u32 {
            let argb = 0xFF00_0000 | 0xFF << 16 | i;
            assert!(hue_of(argb).unwrap() < 360);
        }
    }

    #[test]
    fn follows_the_wallpaper_only_when_asked() {
        let mut prefs = Prefs::default();
        assert!(prefs.system_accent);
        assert_eq!(pick_hue(&prefs, Some(140)), 140);
        assert_eq!(pick_hue(&prefs, None), prefs.accent_hue);
        prefs.system_accent = false;
        assert_eq!(pick_hue(&prefs, Some(140)), prefs.accent_hue);
    }

    #[test]
    fn a_preset_is_drawn_in_its_own_hue() {
        let css = include_str!("../assets/main.css");
        assert!(css.contains("oklch(0.6 0.16 var(--swatch-hue))"));
        // Some fall outside sRGB, and clipping them moves the hue a little.
        for hue in [285, 330, 20, 65, 140, 190, 245, 0] {
            let [r, g, b] = swatch_rgb(hue);
            let got = hue_of(u32::from_be_bytes([0xFF, r, g, b])).unwrap();
            let apart = got.abs_diff(hue).min(360 - got.abs_diff(hue));
            assert!(apart <= 10, "{hue}: {got}");
        }
    }

    #[test]
    fn the_page_background_fits_srgb_at_every_hue() {
        // Regression: WebView 113 painted the page yellow on a bluish wallpaper,
        // because the blue channel of the canvas came to 256 and wrapped to 0.
        let css = include_str!("../assets/main.css");
        let canvases: Vec<_> = css.split("--canvas: oklch(").skip(1).collect();
        assert!(!canvases.is_empty());
        for canvas in canvases {
            let mut parts = canvas.split_whitespace();
            let l: f64 = parts.next().unwrap().parse().unwrap();
            let c: f64 = parts.next().unwrap().parse().unwrap();
            for hue in 0..360 {
                let rgb = srgb(l, c, f64::from(hue));
                assert!(rgb.iter().all(|&ch| ch <= 1.0), "{l} {c} {hue}: {rgb:?}");
            }
        }
    }
}
