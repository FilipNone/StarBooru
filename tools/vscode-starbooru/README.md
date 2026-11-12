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
| Docker stack | The production image, compiled from this checkout, via `docker compose … up --build` on port 8090. Creates `.env` with random secrets if it is missing | your browser |

Each target runs in its own terminal, named `StarBooru: <target>`. Click a running target to see its output.
- **■** stops the target; closing its terminal does the same.
- **⟳** restarts it.
- **⧉** opens it again.
- **⚙** (on a stopped target, or right-click any of them) changes its port.
- **Stop Everything** in the view title stops everything.

Stopping the Docker stack runs `docker compose stop`, so its volumes are kept.

Every action is also in the Command Palette under **StarBooru:** (Start, Stop, Restart, Open, Change Port…, Stop Everything).

## Ports

A stopped target shows its port next to its name. If another program already holds that port, ▶ says so before
anything starts and offers **Change Port…**; the start goes on with the new port. The new port is saved in your user
settings (or in the workspace, if the port is set there). If the target is running, the runner offers to restart it
and whatever runs on top of it: the server for a database port, the apps for a server port.

The database container's port is fixed when the container is made. After a change, the next start removes
`starbooru-dev-pg` and makes it again on the new port. The data is in the `starbooru-dev-pgdata` volume, so it stays.

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
- `starbooru.serverPort`, `starbooru.webPort`, `starbooru.dockerPort`, `starbooru.databasePort` (1024–65535; ⚙ sets them for you).

The extension is plain JavaScript because VS Code extensions run on Node. It uses Node built-ins
only: no `node_modules` and no build step.
