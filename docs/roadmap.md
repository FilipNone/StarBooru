# Roadmap

_Status as of 2026-10-28. Scope comes from `docs/checklist.md`._

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
3. **M3, admin and safety.** Done: sign-in with an OpenID Connect provider, login rate limiting, API keys, admin settings, server stats, usage stats and limits, and database backups.
4. **M4, native polish.** Caching and offline use, a keyring token store, Android dynamic colour, in-app updates, multiple booru backends, sync.
5. **M5, AI.** Done: automatic tagging (meta tags from the file, and AI tag suggestions), OCR and translation (also of comic and manga pages, drawn over the image), with a bring-your-own OpenAI-compatible provider or local AI through Ollama (presets from a Raspberry Pi to a desktop GPU, with a hardware check and speed test), chosen per tool. Left: embeddings with pgvector for similar-image search (and content-aware related media), upscaling and background removal.

## Server (docker)

| Item | Status | Notes |
|---|---|---|
| docker | ✅ | `Dockerfile`, `docker-compose.yml` for Dokploy, healthcheck, non-root user |
| blazingly fast database | ✅ | Tags as an `int[]` with a GIN index, partial indexes, counts kept in step. Pages build full rows only for their own ids. Benchmarked at a million posts: browsing and tag searches take 3–19 ms, Explore 72–116 ms (`development.md` › *Speed at scale*). |
| tag system | ✅ | Normalisation, categories, autocomplete (also by alias), aliases (adding one merges an existing tag), transitive implications applied to old and new posts, cycle detection. A Tags tab lists tags by use or name, and each tag has a page to edit its category, aliases and implications. |
| score system | ✅ | Up/down votes, `order:score` |
| feature rich and fast sorting | ✅ | Newest, oldest, score, file size, resolution, tag count, last edit and random, in either direction (`_asc`/`_desc`). Each sort has a partial index on live posts. A sort menu sits in the gallery header. |
| grid view of media | ✅ | |
| pages and infinite scroll | ✅ | Infinite scroll with a "Load more" fallback. No numbered pages yet. |
| media viewer | ✅ | Image, GIF, video. Zoom (wheel, double-click, pinch, buttons, `+`/`-`/`0`) and drag to pan; previous/next in the order of the search you came from (arrows, `←`/`→`, swipe); fullscreen (`F`); `E` edits tags, `Esc` steps back out. Autocomplete takes `↑`/`↓`, `Enter`/`Tab` and `Esc`. |
| related media suggestion | ✅ | The viewer shows up to 12 related posts under the details. They share the post's rarer tags (each shared tag counts by how rare it is) or look like it (a difference hash within a few bits), so look-alikes show even without tags. Found from the 8 rarest tags and a 4-chunk hash index, never a scan. Image embeddings (M5) could later add posts that look alike only in content. |
| simple image editor / simple video editor | ✅ | The post viewer's editor turns, flips and crops an image (drag the corners) or trims a video (type times or take the playing position). The result is saved as a new post with the same tags; the original can go to the recycle bin. Video trims need ffmpeg: a lossless copy when the keyframes allow it, a re-encode otherwise. |
| video preview | ✅ | Thumbnails for video, AVIF and JPEG XL through ffmpeg (bundled in the image), length badge, muted preview on mouse hover, plays in the viewer. Older posts get thumbnails at the next start. |
| media importer / upload tab | ✅ | Multi-file drag and drop; folder import (only images and videos, optionally tagged with the folder name); import from URLs (downloaded by the server, public addresses only by default); shared tags and rating plus tags per file. |
| saved search | ✅ | Star a search in the gallery header to save it, or tap it again to forget it. Saved searches show as chips on the unfiltered gallery. Per user, at most 200. |
| automatic tagging, AI features, OCR | 🟡 | **Automatic tags:** every new post gets Danbooru's meta tags from the file (`lowres`, `highres`, `absurdres`, `wide_image`, `tall_image`, `animated`, `video`, `sound`, `long_video`), once per post so a removed tag stays removed; older posts are tagged at the next start; Administration › Server or `STARBOORU_AUTO_TAGS` turns it off. **AI:** an admin sets a hosted OpenAI-compatible provider (OpenAI, OpenRouter, Gemini) and/or local AI through Ollama in Administration › Server › AI or by environment, and picks for each tool (tagging, translation) off, hosted or local. Local AI has three Qwen3-VL presets (Raspberry Pi, laptop with integrated graphics, desktop with a graphics card); the card reads the server's hardware, says for each whether it runs well, slowly, too slowly or doesn't fit, downloads models with progress and times a test request. *Suggest tags* in the tag editor asks a vision model about the thumbnail and shows chips to add (new tags marked), with a suggested rating; optionally new uploads are tagged in the background. **OCR:** tesseract on the server reads the text in still images after upload (and older ones at start); the viewer shows it with Copy and, with a provider, *Translate* into the app's language; `text:word` / `text:"two words"` search it. **Comics and manga:** the viewer's translate button sends the page to the translation model and draws each balloon's translation over it (hover for the original), kept per language; new uploads with text can be translated automatically into a set language; the image has Japanese OCR (horizontal and vertical). Not done: embeddings (pgvector), upscaling, background removal. |
| explore tab with trending | ✅ | An Explore tab: trending posts (net upvotes in the last day, week, month or year), popular tags on recent posts, *On this day* from earlier years, and shuffleable random picks. A search narrows it all. |
| login system | ✅ | Accounts, Argon2id, revocable tokens, first user becomes admin. Failed sign-ins are limited per account and address (429 with `Retry-After`). Optional sign-in with an OpenID Connect provider (Authentik, Keycloak, Google…): PKCE, accounts made on first sign-in when registration is open, and provider accounts linked and unlinked in Settings. The desktop app gets the answer through the system browser and a loopback listener. |
| profile settings | ✅ | Theme, accent and language; password change (signs out other devices); a list of signed-in devices with last activity, sign-out per device and *Sign out everywhere else*. |
| usage stats / usage limits | ✅ | Each account sees its uploads (posts and bytes) in Settings, with a meter when it has a limit. Admins see every account's usage and set a storage limit per account; uploads past it are refused (`quota_exceeded`). |
| automatic smart image compression | ✅ | Originals stay byte-exact. Big opaque images (1 MiB and up) also get a JPEG copy of at most 2560 px, kept only when it saves a quarter or more (a 16 MB PNG became 477 KB). The viewer and share pages show the copy, and switch to the original when you zoom in or tap *Show original*; a setting always opens originals. Older posts get copies at the next start. `STARBOORU_SAMPLES=false` turns it off. |
| video reproduction offload to client | ✅ | Files are streamed with Range support. There is no server transcoding. |
| mass edits | ✅ | M2 |
| administration settings tab / server stats tab | ✅ | Settings › Administration (admins): *Overview* with post, tag, account and storage totals, database size and uploads per day for 30 days; *Accounts* to change roles, disable, reset passwords, set storage limits and delete; *Server* for the name, open registration and recycle-bin days, which apply at once over the environment variables. |
| database backup | ✅ | `pg_dump` archives on a schedule (daily, keep 7 by default) or by hand from Administration › Backups, with download, delete and the schedule there. Restore is a documented `pg_restore` step. |
| recycle bin to recover files | ✅ | Soft delete and restore, a recycle bin page, permanent delete per post, *Empty recycle bin* (admin), optional automatic purge (`STARBOORU_TRASH_DAYS`) |
| dark / light theme, custom main colour | ✅ | System, light and dark themes. Accent from swatches or a hue slider. |
| share system | ✅ | *Share* on a post or on a gallery selection makes a link (`/s/…`) for up to 500 posts, with an optional title and a lifetime of a day, a week, a month or until removed. Anyone with it sees the media (no tags or account details) in a grid with a full-screen viewer, without signing in. Settings › Shared links lists each link with its cover, opens count and expiry, to copy again or remove. |
| duplicate remover | ✅ | Exact duplicates are refused at upload (SHA-256). Settings › Duplicate finder groups near-duplicates (difference hash plus average colour, Strict/Normal/Loose), keeps the best copy with optional tag merge and bins the rest, or marks a group as not duplicates. |
| api key | ✅ | Settings › API keys: named keys, full or read-only, never expiring or for 30/90/365 days, with last use shown. The token is shown once and stored hashed. Keys can't manage sign-ins, the password or other keys. |
| sweet animations | ✅ | Spring transitions and a staggered grid. Opening a post grows its thumbnail into the viewer (View Transitions, where the browser or webview has them). The viewer box takes the image's shape at once with the thumbnail inside while the full image loads, and previous/next slide in from their side, with the neighbours fetched ahead so they show at once. All of it is off under reduced motion. |
| language translation future proof | ✅ | Every UI string is in a Fluent catalogue (`apps/app/locales/*.ftl`), with English and Italian. Settings › Language picks one or follows the system. A test checks that every catalogue has every message with the same variables. Server error messages are still English. |

## Desktop app (Windows / Linux / macOS)

| Item | Status | Notes |
|---|---|---|
| login to the self-hosted instance | ✅ | Session stored in the user config directory |
| multiple booru api supported | ✅ | *Upload › Other boorus*: search Danbooru, Gelbooru 0.2 (Safebooru, Rule34…), Moebooru (yande.re, Konachan), e621/e926 and Blombooru, with presets or any address, and an optional account per site (login or user ID, API key) kept on the device. Pick results and import them with their tags, rating and source page. The server makes the requests (`POST /remote/search`) under the URL import rules; a busy site gives 429, which the client waits out. Verified live on each of those sites except Blombooru, which is tested against the response shape in its source (no instance address to test against). Web, desktop and Android share it. |
| api rate limit aware | ✅ | On `429` the client waits as `Retry-After` asks (up to 30 s; 1 s then 2 s without it) and retries twice before surfacing `RateLimited`. Replayable requests only; streamed uploads are sent once. |
| caching | ✅ | Thumbnails and the images the viewer shows are kept on disk by their SHA-256, so they open at once and survive the 6-hourly change of signed URLs. Least recently used files go past a limit set in Settings (off, 256 MB to 5 GB; 512 MB by default), which also shows the size and can clear it. Videos and big originals (over 64 MiB) stream from the server. |
| drag and drop media | ✅ | |
| download and upload media | ✅ | Upload as on the web. *Download* (a post's file, or a backup in Administration) is saved by the app itself into the Downloads folder, streamed to disk without overwriting a file of the same name, and a notice says where it went. Webviews handle `<a download>` differently per OS, macOS's not at all, so the link is no longer relied on. |
| updates working in-app | ⬜ | M4 |
| sync | ⬜ | M4 |
| all user side features present | ✅ | Same code as the web app |
| dark / light theme, animations | ✅ | |
| language translation | ✅ | English and Italian; same catalogues as the web app |

## Android app

| Item | Status | Notes |
|---|---|---|
| builds and runs | ✅ | Same code. Built with `dx build --android` and run on Android 13 (Waydroid, x86_64): sign-in, gallery, viewer, settings. HTTPS uses the system's trusted CAs; it used to panic on the first request. |
| login | ✅ | Session and settings are kept in the app's private files dir and survive a restart (verified on Android 13). They were lost before, because Android apps have no home directory for `directories` to use. The media cache uses the app's cache dir. |
| mobile optimised layout | ✅ | Bottom navigation, 3-column grid, stacked viewer |
| tablet layout, left/right hand modes, ppi optimisation | ✅ | Between phone and desktop widths (761–1100 px, tablets) the navigation is a rail at the side with its items centred, within reach of a thumb. A *Hand* setting (either, left, right) puts the rail, the viewer's panel and its zoom tools on that side. The viewer shows the smaller copy of a big image only while it has every pixel the screen can show: on a dense or big screen (fullscreen on 4K, a 3× phone) it loads the original. Same layout on the web and desktop. |
| dynamic material you colors | ✅ | On Android 12+ the palette follows the wallpaper's accent (`system_accent1_500`, read through JNI) by default, and it is re-read when the app comes back to the front. *Accent colour* shows it as a *Wallpaper* choice beside the fixed swatches. |
| battery optimizations | ✅ | *Battery saver* setting (Automatic, Always, Never). While it's on, videos don't autoplay, the next post isn't prefetched, gallery hover previews and the screen-density swap to originals are skipped, and animations are cut short. Automatic follows the Battery API (≤20% and not charging) and `navigator.connection.saveData`. Videos pause whenever the app is hidden. |
| everything else | | Same as desktop |

## General

| Item | Status |
|---|---|
| documentation up to date | ✅ `docs/` |
| cheat sheet | ✅ `/help`: every search term with examples that run the search, a box that explains a query as you type, and the viewer's keyboard shortcuts. Linked from Settings and from a failed or empty search. |
| Privacy Policy, Terms of Service pages | ✅ `/privacy` and `/terms`, open without signing in and linked from the sign-in card and Settings. They describe what the software stores; the operator of each server is responsible for it. |

## Known gaps and risks

- **Plaintext token on desktop.** The token is stored in the user's config directory. Moving it to the OS keyring is M4.
- **Orphan files.** Fixed for failed inserts (the files are removed unless another post owns them) and interrupted writes (`tmp/` is cleared at startup). Files left by a crash between the write and the insert are not swept yet.
- **AI key in the database.** An API key entered in Administration › AI is stored as plain text in `server_settings`, so it is also in database backups. Setting it with `STARBOORU_AI_KEY` keeps it out of both.
- **OCR coverage.** GIFs and videos are not read, and a scan that failed or timed out counts as done (`POST /posts/{id}/ocr` redoes it; there is no button for that in the app yet). The Docker image has English and Japanese; other languages need their `tesseract-ocr-*` package.
- **Panel translation.** Quality depends on the model: small local models miss text and place boxes loosely, and may translate sound effects literally. Long webtoon strips are shrunk to 1536 px, so their text may be too small to read; there is no splitting yet. The app has no button to redo a kept translation (`POST /posts/{id}/translations` does). Only the app's language is offered in the viewer.
- **Sideways photos from before EXIF orientation.** Images are now decoded upright, but posts uploaded earlier keep the thumbnail and the width/height they got without it. For such a photo the editor's crop can be refused as "outside the image"; turning it still works. Rebuilding those thumbnails and sizes needs a backfill.
