<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/mark-gold.svg">
    <img src="docs/brand/mark.svg" alt="StarBooru logo" width="150">
  </picture>
</p>

<h1 align="center">StarBooru: Sleek Self-hosted Booru</h1>

<p align="center">Organise and tag your private image and video collection, on your own server.</p>

<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/English-f1c45e?style=for-the-badge" alt="English"></a>
  <a href="README.it.md"><img src="https://img.shields.io/badge/Italiano-1b1342?style=for-the-badge" alt="Italiano"></a>
  <a href="README.zh-CN.md"><img src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-1b1342?style=for-the-badge" alt="简体中文"></a>
  <a href="README.ja.md"><img src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-1b1342?style=for-the-badge" alt="日本語"></a>
  <a href="README.ru.md"><img src="https://img.shields.io/badge/%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-1b1342?style=for-the-badge" alt="Русский"></a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--or--later-f1c45e?style=flat-square" alt="License"></a>
  <a href="https://github.com/FilipNone/StarBooru/releases"><img src="https://img.shields.io/badge/releases-download-f1c45e?style=flat-square" alt="Releases"></a>
  <a href="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml"><img src="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1b1342?style=flat-square&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://dioxuslabs.com"><img src="https://img.shields.io/badge/Dioxus-1b1342?style=flat-square" alt="Dioxus"></a>
  <a href="https://github.com/tokio-rs/axum"><img src="https://img.shields.io/badge/axum-1b1342?style=flat-square" alt="axum"></a>
  <a href="https://www.postgresql.org"><img src="https://img.shields.io/badge/PostgreSQL-1b1342?style=flat-square&logo=postgresql&logoColor=white" alt="PostgreSQL"></a>
  <a href="https://www.docker.com"><img src="https://img.shields.io/badge/Docker-1b1342?style=flat-square&logo=docker&logoColor=white" alt="Docker"></a>
</p>

<p align="center">
  <a href="#showcase">Showcase</a> · <a href="#features">Features</a> · <a href="#get-started">Get started</a> · <a href="#roadmap">Roadmap</a> · <a href="#contributing">Contributing</a> · <a href="#wiki">Wiki</a> · <a href="#license">License</a>
</p>

## Showcase

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/gallery-light.webp"><img src="docs/showcase/gallery-dark.webp" width="100%" alt="The gallery: a grid of pictures with saved searches above it"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/viewer-light.webp"><img src="docs/showcase/viewer-dark.webp" width="49%" alt="The viewer: a picture with its tags and details"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/explore-light.webp"><img src="docs/showcase/explore-dark.webp" width="49%" alt="Explore: trending posts"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/search-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/search-light.webp"><img src="docs/showcase/search-dark.webp" width="49%" alt="Search with tag autocomplete"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/upload-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/upload-light.webp"><img src="docs/showcase/upload-dark.webp" width="49%" alt="Upload page with a queue of files"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/phone-gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/phone-gallery-light.webp"><img src="docs/showcase/phone-gallery-dark.webp" width="32%" alt="Gallery on a phone"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/phone-viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/phone-viewer-light.webp"><img src="docs/showcase/phone-viewer-dark.webp" width="32%" alt="Viewer on a phone"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="docs/showcase/phone-explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="docs/showcase/phone-explore-light.webp"><img src="docs/showcase/phone-explore-dark.webp" width="32%" alt="Explore on a phone"></picture>
</p>

## Features

- **Upload your way:** drag and drop, whole folders or web addresses. Files are recognised by their content, so a picture with no extension still gets in.
- **Bring your library:** import from szurubooru, Shimmie2, Danbooru, Stash, Blombooru, Monbooru and LANBooru with tags, ratings and sources, or pick posts from Danbooru, Gelbooru, Moebooru and e621.
- **Tags with structure:** categories, a parent and child tree, aliases, implications, tags per person in a picture, and typo correction.
- **Fast search:** by tag, rating, date, size, colour and text in the image, with autocomplete and saved searches. Tag searches take 3–19 ms at a million posts.
- **A viewer made for looking:** zoom, fullscreen, keyboard shortcuts, video, votes and related posts. Crop, turn and flip pictures, or trim videos.
- **Text and AI, all optional:** OCR for text search, tag suggestions, and comic and manga translation drawn over the page, on local models or any OpenAI-compatible API.
- **Safe with your files:** originals stay byte-exact, duplicates are caught, deleted posts go to a recycle bin, and database backups run on a schedule.
- **Ready for others:** accounts, storage limits, OpenID Connect sign-in, API keys, share links with an expiry, and server statistics.
- **One app everywhere:** web, Windows, Linux, macOS and Android from one codebase, with light and dark themes, your own accent colour and Material You. The interface is in English and Italian.

Formats: JPEG, PNG, WebP, BMP, TIFF, GIF, AVIF, JPEG XL, MP4, WebM, MKV and MOV.

## Get started

Every app connects to your own server, so set that up first. [Docker](#docker) is the quickest way.

> [!NOTE]
> The first release isn't out yet, so [Releases](https://github.com/FilipNone/StarBooru/releases) is empty for now. Until then, build the apps from [source](#source).

### Desktop

| System | Package |
|---|---|
| Windows | `.msi` |
| macOS | `.dmg` |
| Linux | `.deb`, `.AppImage` |

Download from [Releases](https://github.com/FilipNone/StarBooru/releases), start the app and enter your server's address. Daily use and testing happen on Linux; Windows and macOS share the code but haven't been run yet.

### Mobile

Android: install the `.apk` from [Releases](https://github.com/FilipNone/StarBooru/releases) and enter your server's address. It has been run on Android 13. There is no iOS app.

### Server

To run without Docker you need [PostgreSQL](https://www.postgresql.org) 13 or newer and the [source tools](#source):

```sh
cargo build --release -p starbooru-server
dx bundle --web --release -p starbooru-app --out-dir dist

export DATABASE_URL=postgres://starbooru:password@127.0.0.1:5432/starbooru
export STARBOORU_SECRET=at-least-32-characters-kept-the-same
STARBOORU_WEB_DIR=$PWD/dist/public ./target/release/starbooru-server
```

ffmpeg (video thumbnails), tesseract (text in images) and pg_dump (backups) are optional. Every setting is in [docs/deployment.md](docs/deployment.md#configuration).

### Docker

One image holds the server and the web app, next to a PostgreSQL container:

```sh
git clone https://github.com/FilipNone/StarBooru.git
cd StarBooru
cp .env.example .env   # set POSTGRES_PASSWORD and STARBOORU_SECRET
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d --build
```

The first build takes several minutes. Open http://localhost:8080 and create an account: the first one becomes admin.

For Dokploy, graphics cards and reverse proxies, see the [installation guide](https://github.com/FilipNone/StarBooru/wiki/Installation).

### Source

You need [Rust](https://rustup.rs) 1.88 or newer and the Dioxus CLI. On Linux the desktop app also needs WebKitGTK, and Android builds need the Android SDK and NDK.

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli@0.7.10 --locked

dx bundle --desktop --release -p starbooru-app
tools/brand/android-build.sh --release --target aarch64-linux-android
```

The rest, from running each client to the tests, is in [docs/development.md](docs/development.md).

## Roadmap

- [x] Foundation
- [x] Daily driver
- [x] Admin and safety
- [x] Tagging overhaul
- [ ] Native polish
  - [x] Media cache
  - [x] Other booru backends
  - [x] Android dynamic colour
  - [ ] Token in the OS keyring
  - [ ] In-app updates
  - [ ] Offline use
  - [ ] Sync
- [ ] AI
  - [x] Automatic tagging
  - [x] OCR
  - [x] Translation
  - [ ] Similar-image search
  - [ ] Upscaling
  - [ ] Background removal

Details: [docs/roadmap.md](docs/roadmap.md).

## Contributing

Bug reports, ideas and pull requests are welcome in [Issues](https://github.com/FilipNone/StarBooru/issues). Start with [CONTRIBUTING.md](CONTRIBUTING.md); [AGENTS.md](AGENTS.md) has the project's rules. Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Wiki

Guides for using and running StarBooru are in the [wiki](https://github.com/FilipNone/StarBooru/wiki). The [architecture](docs/architecture.md) and the [HTTP API](docs/api.md) are documented in `docs/`.

## License

[AGPL-3.0-or-later](LICENSE). Third-party assets are listed in [ATTRIBUTION.md](ATTRIBUTION.md), and the pictures in the screenshots are public-domain works from the [Cleveland Museum of Art](https://www.clevelandart.org/open-access).

StarBooru is an unofficial personal project, not affiliated with or endorsed by TYPE-MOON or Aniplex.
