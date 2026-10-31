# tao 0.34.8, patched

This is [tao](https://github.com/tauri-apps/tao) 0.34.8 from crates.io (Apache-2.0 or MIT, see `LICENSE`) with one upstream change applied: [tao#1218](https://github.com/tauri-apps/tao/pull/1218), *fix(wayland): fix client-side decorations and apply them only when necessary* (released in tao 0.36). The diff is `wayland-csd.patch`. The examples were left out.

Without it, tao draws its own title bar on Wayland under an event box that swallows clicks, so the window's buttons don't respond (dioxus#3667). Dioxus works around that by running the desktop app through XWayland. With the fix, the app runs natively on Wayland: KDE draws its own title bar, GNOME gets GTK's.

There is one change of our own, `android-exit.patch`: leaving the event loop on Android ends the process with `_exit` rather than `exit`. Closing the app with the back key otherwise ran the destructors of the system's libraries while hwui's threads were still using them, which aborts the process (`pthread_mutex_lock called on a destroyed mutex`).

The root `Cargo.toml` points crates.io's `tao` here with `[patch.crates-io]`. Remove this folder and that entry once `dioxus-desktop` depends on tao 0.36 or later and exits cleanly on Android.
