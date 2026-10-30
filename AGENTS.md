# AGENTS.md: guide for AI agents and contributors

StarBooru is a self-hosted personal booru: organise and tag a private image and video collection.
Everything is **Rust**:
- **Server:** axum and PostgreSQL. It also serves the web app.
- **One Dioxus app:** a single codebase that builds the web interface, the Windows/Linux/macOS desktop app and the Android app.

All three clients talk to the same server over its JSON API. The product name is written **StarBooru** in user-facing text. Machine names stay lowercase: crates `starbooru-*`, binaries, ids (`eu.filipnone.starbooru`).

Read this file first, then `docs/`.

## 1. Orientation

| Read | Why |
|---|---|
| `docs/architecture.md` | Crate map, data model, how search, storage and auth work |
| `docs/development.md` | Toolchain, running the server and every client, tests, visual checks |
| `docs/deployment.md` | Docker image, Dokploy setup, configuration, backups |
| `docs/api.md` | The HTTP API the clients use |
| `docs/roadmap.md` | What is done, partial or missing against `docs/checklist.md`, and known gaps |
| `docs/checklist.md` | The owner's feature backlog (source of truth for scope; don't rewrite it) |
| `log/devlog.md` | Local dev log (gitignored): what landed, what's still open |

## 2. Workspace map

```text
crates/
  core      shared types (API models, tag rules, search query parser); no I/O, builds for wasm
  client    typed async HTTP client for the API (reqwest; wasm + native)
apps/
  server    starbooru-server: axum API, media storage, thumbnails, serves the web app
  ai-worker starbooru-ai-worker: runs local AI models in a process of its own (ONNX Runtime)
  app       starbooru-app: Dioxus UI for web, desktop and Android (features web/desktop/mobile)
tools/
  ai-catalog         pins each AI model file to a commit and a SHA-256 (writes crates/core/src/ai_files.rs)
  vscode-starbooru   VS Code "play button" runner (plain JS: VS Code extensions run on Node; keep it dependency-free)
  nested-kwin.sh     throwaway headless KWin with a private D-Bus session, for UI tests that need a compositor
```

Dependency direction: `core` ← `client` ← `app`, and `core` ← `server`, `core` ← `ai-worker`. The server never depends on
`client` or `app`; `core` and `client` must keep compiling for `wasm32-unknown-unknown`.

## 3. Golden rules

### Never crash

People trust StarBooru with their collection. A malformed upload, a bad query, a corrupt settings file or a full disk must produce an error the user can act on, never a panic.

- **Non-test code never panics.** No `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!` or `unimplemented!`. The workspace lints deny them, and `clippy.toml` allows them in tests only. Return the error type and propagate with `?`. Use `unwrap_or*` only where a fallback is truly correct.
- **No `unsafe`.** The workspace sets `unsafe_code = "forbid"`.
- **Input-derived numbers are hostile.** Use `get()` instead of indexing, `checked_*`/`saturating_*` arithmetic, and cap allocations by configuration. The image decoder runs with an allocation limit.
- **Server handlers return `AppError`.** Internal errors are logged and returned as a generic 500 without details. `CatchPanicLayer` is a last-resort net, not permission to panic.
- **Every crash fix comes with a regression test** that failed before the fix.

### Project rules

1. **The API is the contract.** Request and response types live in `starbooru-core` and are shared by server and clients. Add or change an endpoint in all three places in the same change, and update `docs/api.md`.
2. **Database changes are new migrations** (`apps/server/migrations/NNNN_name.sql`). Never edit an applied migration. They run automatically at startup.
3. **Media is content-addressed and immutable.** Files are named by SHA-256. Deletion is soft (the recycle bin). Clients reach media only through signed `/media` URLs returned by the API.
4. **The UI talks only to the API.** No server code in the app. Platform specifics, such as storage paths and window setup, go behind `cfg` in `store.rs` and `main.rs`. The web build must keep working.
5. **Design tokens, not literals.** Colours come from the CSS role variables in `apps/app/assets/main.css`, all derived from one seed hue. Glass (blur) is used only on the bars and the sign-in card. Respect `prefers-reduced-motion`.
6. **Verify UI changes visually**:
   1. Build the web bundle and serve it from a local server (`docs/development.md` › Visual checks).
   2. Screenshot light and dark themes at desktop and phone (390px) width.
   3. Look at the PNGs.

   Test images must be generated or public domain, never personal photos.
7. **Tests are the gate.** Every change comes with tests. Parsers and validators (`core`) get unit tests. Server behaviour gets tests or a documented manual check in the dev log.
8. **Privacy.** Collections are private. Never log file contents, tokens or passwords. Network requests from tooling use a generic User-Agent and never carry personal details.
9. **Assets.** Third-party assets must be permissively licensed and get a row in `ATTRIBUTION.md`. Never commit font files.
10. **Time goes through `chrono`.**
11. **UI tests never touch the owner's desktop session.** Anything that needs a compositor, `xdotool` or `import -window root` runs inside `tools/nested-kwin.sh`.
    - Never start `kwin_wayland` directly, not even with `--virtual`, and never start one whose D-Bus session is the desktop's. A nested KWin on the desktop's bus broke Alt+Tab and Super+Tab everywhere when it exited.
    - Never stop processes by name (`pkill kwin_wayland`, `pkill -f 'kwin_wayland --virtual'`). Never restart or signal the host compositor.
    - Don't run commands that change the desktop's shortcuts, such as `kglobalaccel` calls, as part of a test.
    - If you find a nested KWin already running on the desktop's bus, don't stop it: its exit is what does the damage. Tell the owner.

    The rules and what to check are in `docs/development.md` › Nested KWin.

## 4. Before you finish a task

```sh
cargo fmt --all
cargo test --workspace --exclude starbooru-app
cargo test -p starbooru-app --no-default-features --features desktop
cargo clippy --workspace --exclude starbooru-app --all-targets -- -D warnings
cargo clippy -p starbooru-app --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings
cargo clippy -p starbooru-app --no-default-features --features desktop -- -D warnings
docker build -t starbooru:dev .      # if you touched the Dockerfile, migrations or build setup
```

Then append a terse entry to `log/devlog.md`: what landed, what you verified, what's still open. Also update `docs/roadmap.md` when a checklist item changes state.

## 5. Tooling notes

- The Dioxus CLI is `dx` 0.7.10 (`cargo install dioxus-cli@0.7.10 --locked`, or the prebuilt release binary as the `Dockerfile` does). It may not be on `PATH`; try `~/.cargo/bin/dx`.
- `dx bundle --web --release -p starbooru-app --out-dir <dir>` writes the site to `<dir>/public`.
- The server needs PostgreSQL. The quickest local one is a throwaway container (`docs/development.md`).
