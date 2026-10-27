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

The runner is plain JavaScript (Node built-ins only, no build step), because VS Code extensions run on Node. It is the one non-Rust part of the repository.

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

## Run the clients

```sh
dx serve --web -p starbooru-app        # hot-reloading web app; point it at http://127.0.0.1:8080
dx serve --desktop -p starbooru-app    # native window
dx serve --android -p starbooru-app    # emulator or device (needs the NDK)
```

Release builds:

```sh
dx bundle --desktop --release -p starbooru-app    # installers for the host OS (.deb/.AppImage, .msi, .dmg)
dx bundle --android --release -p starbooru-app    # .apk / .aab
```

The bundle identifier, publisher and category are in `apps/app/Dioxus.toml`.

The web app keeps the API at its own origin by default. The native apps ask for the server URL at sign-in. Set `STARBOORU_SERVER` to prefill the sign-in page. Native apps read it at run time; the web and Android builds read it at compile time, so set it in the environment `dx serve` runs in. The API allows any origin (it uses bearer tokens, not cookies), so `dx serve --web` on another port works.

## Tests and lints

The gate before finishing a task is in `AGENTS.md` › *Before you finish a task*. While iterating:

Server integration tests run the real router against PostgreSQL. Each test creates its own database and drops it afterwards. Point them at a server whose user may create databases; without the variable they are skipped:

```sh
export STARBOORU_TEST_DATABASE_URL=postgres://starbooru:devpass@127.0.0.1:55432/starbooru
```

The harness is `apps/server/src/testing.rs`. Tests for video, AVIF and JPEG XL thumbnails need `ffmpeg` on `PATH` and are skipped without it.

```sh
cargo test -p starbooru-core
cargo test -p starbooru-server --bins
cargo clippy -p starbooru-app --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings
```

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
