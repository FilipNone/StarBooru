# StarBooru Runner (VS Code)

A play button for every part of StarBooru. Click the **star** in the activity bar, then press ▶ next to
what you want to try. Anything it needs starts first, it waits until it answers, and then it opens.

| Target | What ▶ does | Opens |
|---|---|---|
| Web app | `dx serve --web` on port 8081 (hot reload), after the server | your browser |
| Desktop app | `dx serve --desktop`, after the server | a native window |
| Android app | `dx serve --android` (needs the Android SDK/NDK and an emulator or device), after the server | the app on the device |
| Server | `cargo run -p starbooru-server` on 127.0.0.1:8080, after the database. Registration is on, data goes in `./data` | — |
| Database | PostgreSQL container `starbooru-dev-pg` on 127.0.0.1:55432, data in volume `starbooru-dev-pgdata` | — |
| Docker stack | The production image via `docker compose … up --build` on port 8090. Creates `.env` with random secrets if it is missing | your browser |

Each target runs in its own terminal, named `StarBooru: <target>`. Click a running target to see its output.
- **■** stops the target; closing its terminal does the same.
- **⟳** restarts it.
- **⧉** opens it again.
- **Stop Everything** in the view title stops everything.

Stopping the Docker stack runs `docker compose stop`, so its volumes are kept.

Every action is also in the Command Palette under **StarBooru:** (Start, Stop, Restart, Open, Stop Everything).

The first start compiles everything and can take several minutes; the progress notification
shows while it builds. On the web, the app's sign-in page is prefilled with the dev server. The
first account you register becomes the admin.

## Install

```sh
tools/vscode-starbooru/install.sh          # or: install.sh codium
```

The script packages a `.vsix` with Python's `zipfile` (no npm) and installs it with `code --install-extension`.
Reload the window after a change to `extension.js`.

## Settings

- `starbooru.dxPath`: path to `dx`. Empty means PATH, then `~/.cargo/bin/dx`.
- `starbooru.serverPort`, `starbooru.webPort`, `starbooru.dockerPort`, `starbooru.databasePort`.

The extension is plain JavaScript because VS Code extensions run on Node. It uses Node built-ins
only: no `node_modules` and no build step.
