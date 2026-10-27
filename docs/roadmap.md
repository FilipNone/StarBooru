# Roadmap

_Status as of 2026-10-27. Scope comes from `docs/checklist.md`._

**Legend:**
- ✅ done
- 🟡 partial
- ⬜ not started

## Milestones

1. **M1, foundation (this release).**
   - Server: API, Postgres, storage, thumbnails, signed media, auth.
   - Clients: one Dioxus app for web, desktop and Android.
   - Docker image and a Dokploy-ready compose file.
2. **M2, daily driver.** Video thumbnails, a recycle bin page, mass edit, saved searches, an explore/trending page, a duplicate finder, keyboard navigation, i18n scaffolding.
3. **M3, admin and safety.** Admin settings, server stats, usage limits and stats, API keys, login rate limiting, OAuth, database backup from the UI.
4. **M4, native polish.** Caching and offline use, a keyring token store, Android dynamic colour, in-app updates, multiple booru backends, sync.
5. **M5, AI.** Embeddings with pgvector for related media and similar-image search, automatic tagging, OCR, translation, upscaling and background removal (with a bring-your-own API key).

## Server (docker)

| Item | Status | Notes |
|---|---|---|
| docker | ✅ | `Dockerfile`, `docker-compose.yml` for Dokploy, healthcheck, non-root user |
| blazingly fast database | 🟡 | Tags as an `int[]` with a GIN index, partial indexes, counts kept in step. Not yet benchmarked at 1M posts. |
| tag system | ✅ | Normalisation, categories, autocomplete (also by alias), aliases (adding one merges an existing tag), transitive implications applied to old and new posts, cycle detection. A Tags tab lists tags by use or name, and each tag has a page to edit its category, aliases and implications. |
| score system | ✅ | Up/down votes, `order:score` |
| feature rich and fast sorting | ✅ | Newest, oldest, score, file size, resolution, tag count, last edit and random, in either direction (`_asc`/`_desc`). Each sort has a partial index on live posts. A sort menu sits in the gallery header. |
| grid view of media | ✅ | |
| pages and infinite scroll | ✅ | Infinite scroll with a "Load more" fallback. No numbered pages yet. |
| media viewer | ✅ | Image, GIF, video. Zoom (wheel, double-click, pinch, buttons, `+`/`-`/`0`) and drag to pan; previous/next in the order of the search you came from (arrows, `←`/`→`, swipe); fullscreen (`F`); `E` edits tags, `Esc` steps back out. Autocomplete takes `↑`/`↓`, `Enter`/`Tab` and `Esc`. |
| related media suggestion | ⬜ | M5 (embeddings) |
| simple image editor / simple video editor | ⬜ | |
| video preview | 🟡 | Plays in the viewer. No thumbnail or hover preview yet (needs ffmpeg). |
| media importer / upload tab | 🟡 | Multi-file drag and drop, shared tags and rating. Missing: folder import, per-file tags, import from URL. |
| saved search | ⬜ | M2 |
| automatic tagging, AI features, OCR | ⬜ | M5 |
| explore tab with trending | ⬜ | M2 |
| login system | 🟡 | Accounts, Argon2id, revocable tokens, first user becomes admin. OAuth: M3. |
| profile settings | 🟡 | Theme and accent. No password change yet. |
| usage stats / usage limits | ⬜ | M3 |
| automatic smart image compression | ⬜ | Originals are kept byte-exact by design. Compressed variants could be added. |
| video reproduction offload to client | ✅ | Files are streamed with Range support. There is no server transcoding. |
| mass edits | ⬜ | M2 |
| administration settings tab / server stats tab | ⬜ | M3 |
| database backup | 🟡 | Dokploy Volume Backups (documented). No in-app backup. |
| recycle bin to recover files | ✅ | Soft delete and restore, a recycle bin page, permanent delete per post, *Empty recycle bin* (admin), optional automatic purge (`STARBOORU_TRASH_DAYS`) |
| dark / light theme, custom main colour | ✅ | System, light and dark themes. Accent from swatches or a hue slider. |
| share system | ⬜ | |
| duplicate remover | 🟡 | Exact duplicates are refused at upload (SHA-256). No near-duplicate detection. |
| api key | ⬜ | M3 (tokens exist; long-lived named keys don't) |
| sweet animations | 🟡 | Spring transitions, staggered grid, reduced-motion aware |
| language translation future proof | ⬜ | Strings are inline. Extract them in M2. |

## Desktop app (Windows / Linux / macOS)

| Item | Status | Notes |
|---|---|---|
| login to the self-hosted instance | ✅ | Session stored in the user config directory |
| multiple booru api supported | ⬜ | M4 |
| api rate limit aware | 🟡 | The client surfaces `429` with `Retry-After`. No automatic retry yet. |
| caching | ⬜ | M4 |
| drag and drop media | ✅ | |
| download and upload media | 🟡 | Upload works. Download uses the webview's download handling and is untested on every OS. |
| updates working in-app | ⬜ | M4 |
| sync | ⬜ | M4 |
| all user side features present | ✅ | Same code as the web app |
| dark / light theme, animations | ✅ | |
| language translation | ⬜ | |

## Android app

| Item | Status | Notes |
|---|---|---|
| builds and runs | 🟡 | Same code. The Android build hasn't been run yet (needs the SDK and NDK). |
| login | 🟡 | Works in the UI. Session persistence on Android is unverified. |
| mobile optimised layout | ✅ | Bottom navigation, 3-column grid, stacked viewer |
| tablet layout, left/right hand modes, ppi optimisation | ⬜ | |
| dynamic material you colors | ⬜ | M4 (needs a JNI bridge to read the wallpaper palette) |
| battery optimizations | ⬜ | |
| everything else | | Same as desktop |

## General

| Item | Status |
|---|---|
| documentation up to date | ✅ `docs/` |
| cheat sheet | 🟡 Search syntax in `docs/api.md`. No in-app page yet. |
| Privacy Policy, Terms of Service pages | ⬜ |

## Known gaps and risks

- **No login rate limiting.** Put the instance behind HTTPS and use strong passwords until M3.
- **Plaintext token on desktop.** The token is stored in the user's config directory. Moving it to the OS keyring is M4.
- **No video, AVIF or JPEG XL thumbnails.** The client shows a placeholder.
- **Orphan files.** Fixed for failed inserts (the files are removed unless another post owns them) and interrupted writes (`tmp/` is cleared at startup). Files left by a crash between the write and the insert are not swept yet.
