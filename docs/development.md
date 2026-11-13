# Development

## Toolchain

- Rust 1.88 or newer (developed on 1.98), plus the wasm target: `rustup target add wasm32-unknown-unknown`.
- The Dioxus CLI 0.7.10: `cargo install dioxus-cli@0.7.10 --locked`. It may land in `~/.cargo/bin/dx` without being on `PATH`.
- Docker, for PostgreSQL and the image.
- Desktop on Linux needs WebKitGTK and its dependencies (see the Dioxus docs).
- Android needs the Android SDK and NDK, with `ANDROID_HOME` and `ANDROID_NDK_HOME` set.

## One click: the VS Code runner

`tools/vscode-starbooru` is a small VS Code extension with a ▶ button for each target:
- the web app, the desktop app and the Android app;
- the server and its database;
- the Docker stack.

Starting a target starts whatever it depends on first. The runner waits until the target answers, then opens it. Install it with `tools/vscode-starbooru/install.sh`, then click the star in the activity bar. Details are in its [README](../tools/vscode-starbooru/README.md).

The steps below are what it runs, if you prefer a terminal.

The runner is plain JavaScript (Node built-ins only, no build step), because VS Code extensions run on Node. Apart from the test harness in [Nested KWin](#nested-kwin), it is the one non-Rust part of the repository.

## Run the server

```sh
# throwaway database
docker run -d --rm --name sb-pg -p 55432:5432 \
  -e POSTGRES_USER=starbooru -e POSTGRES_PASSWORD=devpass -e POSTGRES_DB=starbooru \
  pgvector/pgvector:pg17

export DATABASE_URL=postgres://starbooru:devpass@127.0.0.1:55432/starbooru
export STARBOORU_SECRET=dev-secret-dev-secret-dev-secret-0000
export STARBOORU_BIND=127.0.0.1:8080
cargo run -p starbooru-server
```

Migrations run at startup. The first account you register becomes admin.

To also serve the web app from the server, which is how production works:

```sh
dx bundle --web --release -p starbooru-app --out-dir dist
STARBOORU_WEB_DIR=$PWD/dist/public cargo run -p starbooru-server
```

`cargo run -p starbooru-server -- precompress dist/public` writes the `.br` and `.gz` copies that the image ships. It's optional: without them the server compresses each file as it sends it.

Local AI needs the worker: `cargo build -p starbooru-ai-worker` (`target/debug/` is next to the server; the VS Code runner does this for you). The server downloads the pinned ONNX Runtime into `<data>/ai/runtime/` by itself on Linux x86-64 and ARM64. Elsewhere, point `STARBOORU_ORT_LIB` at a `libonnxruntime` 1.31.

## Run the clients

```sh
dx serve --web -p starbooru-app        # hot-reloading web app; point it at http://127.0.0.1:8080
dx serve --desktop -p starbooru-app    # native window
dx serve --android -p starbooru-app    # emulator or device (needs the NDK)
```

Release builds:

```sh
dx bundle --desktop --release -p starbooru-app    # installers for the host OS (.deb, .msi, .dmg)
tools/brand/android-build.sh --release            # .apk, with the StarBooru icon and name
```

The bundle identifier, publisher and category are in `apps/app/Dioxus.toml`.

Build any Android APK you mean to keep with `tools/brand/android-build.sh`, which takes `dx build` flags. Pass `--target aarch64-linux-android` for a phone: without it the build is x86_64, and the APK keeps whatever arm64 library is left from before. `dx` always writes its default launcher icon and an app label of "StarbooruApp" into the Gradle project, so the script copies `apps/app/android-res/` over them and runs Gradle again. A plain `dx build --android` or `dx serve --android` works for testing and only looks wrong on the home screen.

With a phone, use `adb -s <serial>` (or `ANDROID_SERIAL`) if Waydroid is also listed, and `dx build --android -p starbooru-app --target aarch64-linux-android`. A dozing phone shows a black screenshot and `Page.captureScreenshot` hangs, so wake it first: `adb shell input keyevent KEYCODE_WAKEUP`, then `adb shell wm dismiss-keyguard`. Things only a device shows, to check by hand when they change, because unit tests can't reach them:
- Tap a tile on a cold cache (`adb shell "run-as eu.filipnone.starbooru rm -rf cache/media"`) with media held back by a proxy for over 10 seconds (`adb reverse tcp:<app's port> tcp:<proxy>`). The app stays up and the tabs still answer.
- Press back at each step, from zoom to the first page. The first page closes the app with nothing new in `adb logcat -b crash`.

Without a phone, [Waydroid](https://waydro.id) works as the device: `adb connect <its IP>:5555`, then `dx build --android -p starbooru-app --target x86_64-linux-android` and `adb install -r` the APK under `target/dx/starbooru-app/debug/android/app/app/build/outputs/apk/debug/`. `adb reverse tcp:8080 tcp:8080` lets the app reach a server on your machine at `http://127.0.0.1:8080`. Debug builds expose the WebView to DevTools: forward `localabstract:webview_devtools_remote_<pid>` with `adb forward` and open `chrome://inspect`. Typing with `adb shell input text` drops characters in the WebView. Clippy for Android needs the NDK's clang: set `CC_x86_64_linux_android` and `AR_x86_64_linux_android` to `x86_64-linux-android24-clang` and `llvm-ar` from `$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin`.

The web app keeps the API at its own origin by default. The native apps ask for the server URL at sign-in. Set `STARBOORU_SERVER` to prefill the sign-in page. Native apps read it at run time; the web and Android builds read it at compile time, so set it in the environment `dx serve` runs in. The API allows any origin (it uses bearer tokens, not cookies), so `dx serve --web` on another port works.

On Linux the desktop app runs natively on Wayland, with WebKit's DMA-BUF renderer on. Dioxus 0.7 instead runs it through XWayland with that renderer off, which copies every frame through memory and makes the app lag on large scaled screens, because tao 0.34 drew a title bar whose buttons didn't respond on Wayland. `vendor/tao` carries tao's fix (see its `STARBOORU.md`), and the old setup is kept only for NVIDIA's driver. `STARBOORU_DMABUF=0` brings it back (try it if the window stays blank) and `=1` forces the renderer on; `GDK_BACKEND` still picks the backend. WebKitGTK halves its frame rate while the system's power profile is *Power saver*. `startViewTransition` aborts WebKitGTK's web process (2.54), so the picture that grows into the viewer is plain script (`apps/app/src/motion.rs`).

## Tests and lints

The gate before finishing a task is in `AGENTS.md` › *Before you finish a task*. While iterating:

Server integration tests run the real router against PostgreSQL. Each test creates its own database and drops it afterwards. Point them at a server whose user may create databases; without the variable they are skipped:

```sh
export STARBOORU_TEST_DATABASE_URL=postgres://starbooru:devpass@127.0.0.1:55432/starbooru
```

The AI worker's tests in `apps/ai-worker/tests/worker.rs` also run models when `STARBOORU_ORT_LIB` and `STARBOORU_AI_MODELS` are set.

The harness is `apps/server/src/testing.rs`. Tests for video, AVIF and JPEG XL thumbnails need `ffmpeg` on `PATH` and are skipped without it.

```sh
cargo test -p starbooru-core
cargo test -p starbooru-server --bins
cargo clippy -p starbooru-app --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings
```

## Speed at scale

`apps/server/src/bench.rs` is an ignored test. It seeds generated posts straight into a test database (one million by default):
- 5 to 24 tags a post, taken from 5,000 tags, with popularity falling off steeply;
- 2% of the posts in the bin;
- votes on 5%;
- a random difference hash on each post, except 1% that share a blank one (hash 0);
- 3 to 8 colour cells a post, drawn towards white, black and grey, with teal the rarest.

It then times the API calls the clients make most, through the real router, and fails if any median is over 400 ms. Run it in release:

```sh
STARBOORU_BENCH_POSTS=1000000 cargo test --release -p starbooru-server search_at_scale -- --ignored --nocapture
```

Set `STARBOORU_BENCH_KEEP=1` to keep the seeded database for `EXPLAIN ANALYZE` (the name is printed; drop it afterwards).

Results at 1,000,000 posts (Ryzen 9 5900X, PostgreSQL 17 in Docker, default settings; median of 5 runs; seeding took 113 s). In the call column, `page=1000` is 60,000 posts deep, and `tag_1` is on half the posts.

| Call | Median |
|---|---|
| `/posts`, `/posts?page=50`, every `order:` but random | 2.5–2.8 ms |
| `/posts?page=1000` | 5.4 ms |
| `/posts?q=tag_1`, `tag_1 tag_2 tag_3`, `tag_2 -tag_1`, `rating:e type:video` | 3.2–4.0 ms |
| `order:longside`, `width:>3900`, `mpixels:>12`, `-width:>1000`, `date:<7d` | 2.4–2.7 ms |
| `ratio:16:9`, `tag_1 width:>1920`, `date:>30d`, `date:2026-10` | 3.8–19 ms |
| `tag_300 date:>30d` | 30 ms |
| `/posts?q=tag_4000` (a rare tag) | 18.8 ms |
| `color:white`, `color:red`, `color:teal`, `color:000080`, `-color:white`, `tag_1 color:blue`, `color:red color:blue`, `color:red order:score` | 3.1–4.1 ms |
| `color:ff00ff` (nothing stored nearby, so it widens the search) | 6.6 ms |
| `tag_300 color:teal` (a rare tag and a rare colour; it varies between runs) | 13–190 ms |
| `tgag_300` (corrected to `tag_300`) | 9.3 ms |
| `qqqqqqqq` (nothing close) | 1.1 ms |
| `/tag-corrections` with three names, two of them unknown | 4.5 ms |
| `/posts?q=order:random` | 141 ms |
| `/posts/{id}`, `/posts/{id}/neighbors`, tag search | under 1.2 ms |
| `/posts/{id}/related` (a random hash, and a blank one shared by 10,000 posts) | 24–37 ms |
| `/explore` | 72 ms |
| `/explore?q=tag_1` | 116 ms |
| `/admin/stats` | 112 ms |

`order:random` sorts every match by `random()`, so it grows with the collection. The admin stats count the whole table.

## Speed on a slow device

The target is a Raspberry Pi 4 showing the web app in Chromium: scrolling the gallery and moving between posts without stutter. Without one at hand, the DevTools protocol can slow the page's main thread (`Emulation.setCPUThrottlingRate`). The figures below used a rate of 6 as a stand-in, on a Ryzen 9 5900X. That rate is a guess at the gap between the two processors and hasn't been checked against a Pi.

The setup was headless Chromium (`--use-angle=swiftshader --enable-unsafe-swiftshader`) at 1280×800, a server with 1500 generated pictures, and the pointer resting over the grid, as a page scrolling under a still pointer is what the hover work depends on. Wheel events came through `Input.dispatchMouseEvent`: 40 px every 16 ms for 25 screens as the steady scroll, and ten bursts of twelve 260 px events as flings. Recorded for each:
- the time between animation frames, where a frame is late past 25 ms;
- tasks over 50 ms (`PerformanceObserver`, `longtask`);
- script, style and layout time from `Performance.getMetrics`;
- a trace (`devtools.timeline`) when a late frame needs explaining.

Medians of five runs, alternating the builds:

| | Before | After | After, reduced effects |
|---|---|---|---|
| Steady scroll, late frames | 11.9% | 2.8% | 2.1% |
| Steady scroll, tasks over 50 ms | 17 | 0 | 0 |
| Steady scroll, main thread busy | 6.7 s | 4.3 s | 3.7 s |
| Flings, late frames | 30.8% | 12.2% | 9.5% |
| Flings, tasks over 50 ms | 31 | 0 | 0 |
| Twelve posts forward, late frames | 43.2% | 42.9% | 7.7% |
| Opening a post, longest task | 323 ms | 304 ms | 288 ms |
| Back to the gallery, longest task | 177 ms | 155 ms | 127 ms |

Without the slowdown the same scroll went from 1.9% to 1.3% late frames, and the flings from 8.9% to 0.8%.

What these numbers leave out:
- **The frames are the main thread's.** Wheel scrolling is drawn by the compositor thread, which the slowdown doesn't touch. A late main frame delays new tiles and the hover, while the page keeps moving. Chrome's own scroll records (`ScrollJankV4`, in a trace with the `cc`, `benchmark` and `input` categories) counted no janky update before or after.
- **The graphics chip isn't modelled.** Headless Chromium draws in software, where the blur behind the glass and the viewer's glow cost far more than on a desktop card, and a Pi's chip has limits of its own. Flipping through posts with full effects is slow here for that reason: the GPU thread's work falls by two thirds with reduced effects. `pace.rs` measures this on the device itself and reduces the effects there.
- **Runs differ.** The same build gave 9% and 33% late frames minutes apart while a compile was running. Alternate the builds and compare medians.

On the first visit over a slow link (60 ms, 10 Mbit/s, same slowdown) the first thumbnail showed after 1.81 s instead of 2.03 s, with 1192 kB transferred instead of 1398 kB. A reload took the same time before and after.

### Opening a post

A count of late frames over three seconds hides when they fall, and a frame lost while the picture is arriving shows where one lost on a still page doesn't. So this one is a timeline from the click: each late frame, each long task, when the related posts went onto the page and when each of their pictures loaded.

Headless Chromium can draw with the machine's graphics card (`--enable-gpu` in place of the two SwiftShader flags). It opens no window and needs no compositor. With the card, what is left in the timeline is the main thread's, which is what the slowdown models. In software the graphics thread was busy for 0.9 s of the first 1.2 s of an opening whatever the page did, and hid the rest.

Medians of five runs at the slowdown of 6, alternating the builds, late frames by phase:

| | Before | After |
|---|---|---|
| From a related post, while the picture arrives (100 to 800 ms) | 5 | 1 |
| From a related post, afterwards | 0 | 2 |
| From the gallery, the task at the click | 301 ms | 246 ms |
| From the gallery, during the flight (350 to 1200 ms) | 9 | 4 |
| From the gallery, afterwards | 0 | 0 |

Before, the twelve related posts and their pictures went onto the page about 0.5 s after the click, in the middle of the picture's arrival, together with the two neighbours fetched ahead. Now they wait for the viewer to come to rest (`motion::rested`) and arrive around 1 s, two pictures at a time. That work hasn't gone: it is the two late frames afterwards, on a page where only the related tiles themselves are moving.

### Starting

Over a slow link (80 ms, 4 Mbit/s, same slowdown) the page's first paint is the loading screen of `apps/app/index.html`, 0.14 s after the request. It was a blank page until the app drew, 2.4 s in. The app is on the page at the same moment as before (2.25 s against 2.29 s). On a reload the first paint comes at 0.16 s instead of 0.39 s.

## UI text and translations

Every string the app shows lives in `apps/app/locales/<code>.ftl` ([Fluent](https://projectfluent.org/) syntax) and is looked up with `t!`:

```rust
h2 { {t!("tags-category")} }
p { {t!("upload-skipped", count = skipped)} }
```

- Add a new string to `en.ftl` and to every other catalogue in the same change. `cargo test -p starbooru-app --no-default-features --features desktop` fails if an id is missing from a catalogue, unused, or uses different variables between languages.
- Plurals use Fluent selectors (`{ $count -> [one] … *[other] … }`). Don't build sentences from fragments in Rust.
- To add a language, copy `en.ftl` to `<code>.ftl`, translate it, and add a `Lang` variant (code, name in that language, `include_str!`) in `apps/app/src/i18n.rs`.
- Server error messages are English and are shown as they come.

To check a language visually, set `"language": "it"` in the stored preferences or pick it in Settings › Appearance.

## Visual checks

1. Build the web bundle.
2. Run the server with `STARBOORU_WEB_DIR` pointing at it.
3. Drive it with a headless Chromium or Playwright.

Check the sign-in page, the gallery, a post, upload and settings. Do each in light and dark, at 1280px and 390px wide. Use generated test images (for example solid colours made with ImageMagick or Python), never personal photos.

Four things to know when reading the results:
- **The web page starts from `apps/app/index.html`.** `dx` uses it in place of its own page and writes the links to the stylesheet and the script into it. It holds the loading screen, whose colours are copies of three tokens in `main.css`; a test in `main.rs` fails when they differ. `#main` in it has to stay empty, without even a space, as the loading screen goes when `#main:not(:empty)` matches.
- **Only `backdrop-filter`, without `-webkit-`.** Given both spellings, the minifier in `dx bundle` keeps only the prefixed one, and Chromium and Android get no blur. A debug build doesn't show this.
- **Old stylesheets stay in the bundle's `assets/`.** `dx` keeps earlier hashed copies under `target/dx`, so grep the newest `main-*.css`, not the first match.
- **A sharp ghost of the text through the glass** at 2× in headless Chromium comes from its software renderer. `--use-angle=swiftshader --enable-unsafe-swiftshader` and real phones don't show it.

## Making a release

`.github/workflows/release.yml` builds every download on a push of a `v*` tag and puts them in a draft release. A hyphen in the tag (`v0.1.0-beta.1`) makes it a prerelease. Running the workflow by hand builds everything without releasing, which is how to try a change to the workflow; the filenames then say `main`.

Cargo's version stays `0.1.0`, as a pre-release name can break the Windows installer. What the apps and the server show comes from `STARBOORU_VERSION`, which both workflows set to the tag at build time (`starbooru_core::VERSION`). A local build without it shows Cargo's.

`.github/workflows/image.yml` builds the server image from the same tag and pushes it to `ghcr.io/filipnone/starbooru` under its version: x86-64 and ARM64, plus `<version>-vulkan` and `<version>-nvidia`. Publishing the release moves `latest`, and `latest-vulkan` and `latest-nvidia` with it, which is what every install runs, so a draft reaches nobody. A beta moves them only until there is a stable release. Running it by hand builds an existing tag again, with `latest` moved only if you tick that.

It pins the Rust version of the `Dockerfile`, as newer linkers strip a section wasm-bindgen needs. The Linux builds use the oldest supported Ubuntu, so they run on any newer glibc. Each desktop job starts the app and uploads a screenshot as `screenshot-*`: look at them. That job starts the binary it built, so also install the `.deb` in a clean container to catch a missing dependency. There is no AppImage: the one dx makes leaves out WebKit's helper processes and can't start.

The APK is signed on the owner's machine, so the key never reaches GitHub. Download the `android-unsigned` artifact of the run, then:

```sh
K=~/.config/starbooru-release
gh run download <run id> -n android-unsigned -D apk
zipalign -f -p 4 apk/app-release-unsigned.apk aligned.apk
apksigner sign --ks "$K/starbooru-release.jks" --ks-pass "file:$K/keystore-password" \
  --ks-key-alias starbooru --out StarBooru-<version>-android-arm64.apk aligned.apk
apksigner verify --print-certs StarBooru-<version>-android-arm64.apk
```

The certificate's SHA-256 must be `fb4481c6c698fb00a43085ce002f79baf14f7df0ba0de3e009cf02ebda31de16`. Android refuses an update signed with any other key, and the only way past that is to uninstall the app, which deletes its settings, so don't lose the key. For the same reason, a phone with a debug build has to uninstall it before installing a release.

Upload the APK and a `SHA256SUMS` of every file and write the notes. Wait for the Image workflow to finish, as publishing needs its images, then publish with `gh release edit <tag> --draft=false`. Add `--prerelease` for a beta.

## Nested KWin

The desktop app, real wheel and click events through `xdotool`, and root screenshots need a compositor. Never use the one you are working in. Run the test through `tools/nested-kwin.sh`, which starts a headless KWin and runs your command inside it:

```sh
tools/nested-kwin.sh -- ./my-test.sh
tools/nested-kwin.sh --x-display :77=1280x800 --x-display :78=2560x1600 --log kwin.log --timeout 300 -- ./my-test.sh
```

`--x-display` adds a rootful Xwayland, which `xdotool` and `import -window root` need. The first one becomes `DISPLAY`; address another with `DISPLAY=:78`. The command's exit status is returned. The other options and exit codes are at the top of the script.

Why it exists: a `kwin_wayland --virtual` started directly joins the desktop's D-Bus session. It logs `Failed to register service org.kde.kglobalaccel`, but still registers the same `kwin` shortcut component as the real compositor. When it exited, Alt+Tab and Super+Tab stopped working everywhere: the real component reported `isActive() == false` while keeping its bindings. It also wrote to the real `~/.config`, adding a `Virtual-0` output to `kwinoutputconfig.json` and a tiling section to `kwinrc`.

What the harness does:
- **Private bus:** KWin and your command run under `dbus-run-session`. Before your command starts, the harness checks that KWin really owns `org.kde.kglobalaccel` on that bus, and aborts if it doesn't.
- **Temporary settings:** `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` and `XDG_STATE_HOME` point at a temporary directory that is deleted afterwards.
- **No host display:** `--virtual` needs none, so `WAYLAND_DISPLAY`, `DISPLAY`, `XAUTHORITY` and `SESSION_MANAGER` are dropped, then set to the nested values.
- **Refuses collisions:** it won't start on a Wayland socket or X display that already exists.
- **Scoped cleanup:** on exit, error, timeout or `INT`, `TERM` and `HUP` it stops only processes in its own session or carrying its random id in their environment. If the harness itself is killed with `SIGKILL`, a watchdog stops the tree.

`XDG_RUNTIME_DIR` stays the desktop's, because it only holds the uniquely named socket, and `DBUS_SESSION_BUS_ADDRESS` is only replaced inside the harness. Never export either into a normal shell.

Limits, so keep tests inside them:
- `HOME` is the real one. A program that writes to a fixed path under it isn't contained, so give browsers an explicit profile, for example `--user-data-dir="$XDG_CACHE_HOME/chromium"`.
- `$XDG_RUNTIME_DIR/bus` is still reachable. Don't run `systemctl --user` or pass the desktop's bus address inside a test.
- Test scripts that drive a display should refuse to run outside the harness, so a wrong `DISPLAY` can't send clicks to the real desktop:
  ```sh
  [ "${STARBOORU_NESTED_KWIN:-}" = 1 ] || { echo "run this inside tools/nested-kwin.sh" >&2; exit 2; }
  ```

If you change the harness, check it again, without launching anything on the desktop's bus. All of these are read-only on the desktop's side:
1. Inside, `DBUS_SESSION_BUS_ADDRESS` differs from the desktop's, and no process of the harness appears in `busctl --user list`.
2. Before, during and after a run, these are unchanged: the owner of `org.kde.kglobalaccel`, `isActive` and `allShortcutInfos` on `/component/kwin`, and `~/.config/kglobalshortcutsrc`, `kwinrc` and `kwinoutputconfig.json`.
3. `dbus-monitor --session --profile` shows no calls to `org.kde.kglobalaccel` from the run. Always use `--profile`: without it the capture holds message bodies, including other applications' secrets.
4. Nothing is left after a normal exit, a failing command, each signal, a timeout, or `SIGKILL` of the harness or of the nested KWin.

A D-Bus check can't prove the physical keys work, so ask the owner to press Alt+Tab and Super+Tab afterwards.
