# Architecture

```text
 browser ──┐
 desktop ──┼── HTTPS, JSON API (/api/v1) + signed media URLs ──▶ starbooru-server ──▶ PostgreSQL
 android ──┘                                                           │
                                                                       └──▶ /data (originals, thumbnails)
```

One server process does three jobs:
- it serves the JSON API under `/api/v1`;
- it serves media files under `/media`;
- it serves the compiled web app (any other path falls back to `index.html`).

The desktop and Android apps are the same Dioxus code as the web app. Each one asks the user for a server URL when they sign in.

## Crates

| Crate | Role |
|---|---|
| `crates/core` (`starbooru-core`) | API models (`Post`, `Tag`, `User`, …), tag normalisation, the search query parser, the other-booru request and its checks (`remote.rs`). Pure Rust, no I/O. |
| `crates/client` (`starbooru-client`) | `Client`: one async method per endpoint, with a bearer token. Maps HTTP errors to `ClientError`. On Android it trusts the system's CA folder (`roots.rs`), because reqwest's platform verifier needs JNI setup and a Java helper in the APK. A `429` is retried twice after the `Retry-After` wait (at most 30 s) before it surfaces as `RateLimited { retry_after }`. Runs on wasm and native. |
| `apps/server` (`starbooru-server`) | axum router, auth, posts, tags, storage, thumbnails, signed media. |
| `apps/app` (`starbooru-app`) | Dioxus 0.7 UI. Feature `web`, `desktop` or `mobile` selects the renderer. |
| `vendor/tao` | tao 0.34.8 with upstream's Wayland title bar fix, put in place of the crates.io one by `[patch.crates-io]` until Dioxus moves to tao 0.36. Not a workspace member. |

### Server modules

| Module | What it does |
|---|---|
| `config.rs` | Environment variables (see `deployment.md`) |
| `routes/` | One file per resource: `auth`, `posts`, `tags`, `trash`, `duplicates`, `admin`, `backups`, `shares`, and more. `mod.rs` builds the router and its layers (CORS, compression, tracing, panic catcher, SPA fallback). Every request runs in its own task, so a client that disconnects (a closed tab, a phone going to sleep) doesn't cancel a translation, OCR, an edit or an upload that has already arrived; the result is stored as usual and the client sees it on its next load. |
| `auth.rs` | Bearer-token extractor for sessions (`sb_…`) and API keys (`sbk_…`); a read-only key is refused anything but `GET`/`HEAD`, and `AuthUser::session_id()` refuses keys on the account endpoints. Argon2id password hashing (on the blocking pool). |
| `oidc.rs` | OpenID Connect client: discovery (cached for an hour), the authorization URL with PKCE (S256) and nonce, the token request (`client_secret_basic`), and ID-token claim checks (issuer, audience, expiry, nonce). The token comes straight from the provider over TLS, so its signature is not checked. Also the `return_to` rule (own web app or loopback). `routes/oidc.rs` keeps the sign-ins in progress in `oidc_logins` and hands results over as one-time codes bound to a secret handle. |
| `backup.rs` | Database backups: runs `pg_dump` (password in the environment, not the arguments) into the backup folder, one at a time, lists and prunes them, signs their download links, and makes the scheduled ones (checked hourly). |
| `settings.rs` | The server settings in force: the `server_settings` row over the environment variables. Read per request, so admin changes apply at once. |
| `throttle.rs` | Sign-in limits: failed attempts per account and per client address (IPv6 by /64) in fixed windows, in memory and capped at 10,000 keys each. The address is the socket peer, or the last `X-Forwarded-For` entry with `STARBOORU_TRUST_PROXY`. |
| `storage.rs` | Format sniffing, content-addressed storage, thumbnails |
| `similar.rs` | Duplicate finder fingerprints: a 64-bit difference hash (9×8 brightness grid) plus the average colour, and grouping by Hamming distance through a 4×16-bit multi-index, so it never compares every pair. Fine high-frequency patterns blur to flat at 9×8 and can match when their colours do. |
| `ai.rs` | AI provider client: `chat(provider, prompt, image)` posts to an OpenAI-compatible `/chat/completions` (no proxies, 60 s timeout or 5 min for local AI, 1 MiB answer cap, any address since the admin sets it). `AiSetup` routes each tool (tagging, translation) to the hosted provider, the local runtime or nowhere. Tag suggestions from a post's thumbnail, translation of a post's OCR text (text only; `routes/ai.rs`), and background auto-tagging of uploads (two at a time). The key is never logged or returned; the answer parser is `starbooru_core::ai`. |
| `local_ai.rs` | Local AI through Ollama: reads the hardware (`/proc/meminfo`, `/proc/cpuinfo`, `/sys/class/drm`, `nvidia-smi`), lists the runtime's models, downloads one at a time with progress in `AppState`, and times a test request. The presets and the fit check (`assess`) are pure functions in `starbooru_core::local_ai`. |
| `fetch.rs` | URL import download: http(s) only, public addresses only (checked for literal IPs, on every redirect and in a filtering DNS resolver), no proxies, streaming size cap, timeouts |
| `routes/imports.rs` | Background URL imports: `import_jobs` rows claimed oldest first with `FOR UPDATE SKIP LOCKED` by two worker tasks, woken by a `Notify` when a batch is queued (or every minute), each run through the same path as `POST /posts/import`. At startup rows left `running` go back to `queued`; finished rows are deleted after 7 days. |
| `remote.rs` | Other boorus: the search URL for each API (Danbooru, Gelbooru 0.2, Moebooru, e621, Blombooru), with the user's account in the query as that site names it, and the answer turned into `RemotePost`s (absolute http(s) links, valid tags only, each site's rating scale). `routes/remote.rs` fetches it through `fetch.rs`, so the same address rules apply, and never puts the URL (which may carry a key) in an error or log. |
| `ocr.rs` | Text in images: runs `tesseract <file> stdout -l <langs>` as a child process (time-limited, local files only, capped output) on a still image's sample or original, cleans the text (whitespace, noise lines, 10,000 characters at most) into `posts.ocr_text`; after each upload in the background and for unscanned posts at start. The text is never logged |
| `panels.rs` | Comic and manga translation: fits a still image in 1536 px, asks the translation route's vision model for each piece of text with its box and translation (`starbooru_core::panels` builds the prompt and parses the answer), keeps it in `post_translations`; one page at a time, and after OCR finds text in a new upload when automatic translation is on. |
| `probe.rs` | Size, length and a still frame through `ffprobe`/`ffmpeg` child processes (time-limited, local files only, capped output); video trims (stream copy, re-encoded when the copy would start too early) |
| `edit.rs` | Image edits for `POST /posts/{id}/edit`: turn, flip and crop, written back in the original format. `routes/edit.rs` makes the result a new post through the normal upload path. |
| `media.rs` | HMAC-signed media URLs and the file handler (supports Range for video seeking) |
| `error.rs` | `AppError` → JSON `{code, message}` with the right status |
| `testing.rs` | Test-only harness: a fresh database per test and the real router |

## Data model

PostgreSQL schema: `apps/server/migrations/`.

| Table | Contents |
|---|---|
| `import_jobs` | Background URL imports per user: address, source, tags, rating, `state` (`queued`, `running`, `done`, `duplicate`, `failed`), the new `post_id` and a readable `error` |
| `users` | Usernames are case-insensitively unique. Roles are `admin` and `user`; the first account becomes admin. `disabled_at` blocks sign-in and every token; `quota_bytes` caps the bytes of originals the account may upload (checked under a row lock at insert). |
| `user_identities` | Provider accounts (`issuer`, `subject`, unique together) linked to a user, with the provider's `label` and `last_used_at`. `users.password_hash` is NULL for accounts that only sign in this way. |
| `oidc_logins` | Provider sign-ins in progress, at most 10 minutes: hashes of the `state`, the client's handle and the result code, the nonce and PKCE verifier, `return_to`, the account linking it (if any), then the provider's issuer, subject and label once the browser is back |
| `server_settings` | One row of admin overrides (name, open registration, recycle-bin days, automatic meta tags, backup schedule); NULL follows the environment variable |
| `post_translations` | Text drawn over an image, per post and language: blocks (box, original, translation) as JSONB, and the model that made them |
| `sessions` | The SHA-256 of each bearer token (`sb_<64 hex>`) and its expiry, 90 days. The plain token is never stored. Also an `id` for the settings page, `last_used_at` (refreshed at most every 5 minutes) and `client`, a short description like "Firefox on Linux" (`starbooru_core::agent`); the raw User-Agent is not kept. Native apps send `StarBooru-app/<version> (<os>)`. |
| `tags` | Normalised name, category (general / artist / character / copyright / meta) and a `post_count` kept in step inside the same transaction as post edits |
| `posts` | One per file: `sha256` (unique, so duplicates are refused), format, size, dimensions, rating, source, score, `tag_ids BIGINT[]`, `deleted_at` (soft delete), `phash`/`pcolor`, the duplicate finder's fingerprint of the thumbnail, and `meta_tagged_at`, when the automatic meta tags were added (NULL: not yet) |
| `duplicate_dismissals` | Pairs of posts (`post_a < post_b`) marked as not duplicates |
| `post_votes` | One vote per user and post, −1 or +1, with `voted_at` (set when the vote is cast or changed). `posts.score` is their sum. |
| `tag_aliases` | `alias → tag_id`. Aliases are resolved when tagging, searching and in autocomplete. |
| `shares` | Per user: a random 128-bit `token` (kept as is, so the owner can copy the link again), optional `title`, `post_ids BIGINT[]` in display order, optional `expires_at`, `views` and `last_viewed_at`. The public view resolves posts live, so binned posts drop out and come back on restore. |
| `api_keys` | Per user: a name, the SHA-256 of the token, its first 12 characters (`prefix`, for telling keys apart), `read_only`, `last_used_at` (null until used) and an optional `expires_at` |
| `saved_searches` | Per user: a name (unique per user, case-insensitively) and a query string |
| `tag_implications` | `tag_id → implied_id`. Expanded transitively (a recursive CTE) when tags are set, so implied tags are stored on posts. Cycles are refused. Edits take an advisory lock, since they may rewrite many posts. |

### Automatic meta tags

Like Danbooru, the server tags each post with facts it reads from the file, in the `meta` category. The rules are a pure function, `starbooru_core::autotag::meta_tags`, with Danbooru's names and thresholds:

| Tag | When |
|---|---|
| `lowres` | width ≤ 500 and height ≤ 500 |
| `highres` | width ≥ 1600 or height ≥ 1200 |
| `absurdres` | width ≥ 3200 or height ≥ 2400 (also `highres`) |
| `incredibly_absurdres` | width or height ≥ 10000 (also the two above) |
| `wide_image` / `tall_image` | the long side is ≥ 1024 px and ≥ 4 × the short one |
| `animated` | GIFs (every GIF counts, the server doesn't count frames) and videos |
| `video` | videos |
| `sound` | videos with an audio stream (ffprobe; unknown without it) |
| `long_video` | videos of 30 s or more |

Every new post goes through `insert_post` (uploads, URL and booru imports, editor copies), which adds them in the same transaction as the post's own tags: a missing tag is made in `meta`, an existing one keeps its category, aliases and implications apply, `post_count` is kept in step. `posts.meta_tagged_at` is set then, so a post is tagged only once and a tag someone removes stays removed. At startup a backfill tags live posts with `meta_tagged_at IS NULL` (older posts, or ones uploaded while this was off), probing videos for audio. `STARBOORU_AUTO_TAGS` or the admin toggle turns both off.

### Search

Tags are stored on each post as a `BIGINT[]` with a GIN index. A query like `blue_sky night -cloud rating:g order:score` becomes:

1. The tag names are resolved to ids, following aliases. If an included tag doesn't exist, the result is empty, with no scan.
2. The SQL is `tag_ids @> {include} AND NOT tag_ids && {exclude} AND rating = ANY(...)`.
3. Rows are ordered by id, score or random, and paginated by limit and offset (at most 200 per page).
4. The ordered, paginated query selects only post ids. The full rows, with tag names and the caller's vote, are built for the ids on the page (`select_page` in `routes/posts.rs`). So rows an `OFFSET` skips cost nothing extra.

Partial indexes cover live posts, so the recycle bin doesn't slow normal browsing. Explore runs its four sections in parallel. Its random picks come from a sample of the table's pages (`TABLESAMPLE SYSTEM`) once the collection has more than 20,000 posts. When the sample has too few matches for a rare search, all the matches are sorted instead. Timings at a million posts are in `development.md` › *Speed at scale*. The query syntax is in `api.md`.

### Related posts

`GET /posts/{id}/related` ranks live posts by what they share with the post. Each shared tag adds its inverse document frequency, `ln((N + 1) / (post_count + 1))`. So a tag on three posts says a lot and one on half the collection almost nothing. `N` is the planner's row estimate once the table has 10,000 rows, and an exact count below that. A difference hash within 12 bits adds up to 9 more, about what a tag on one post in 10,000 is worth.

Candidates come from two cheap lookups, never a scan:

- the newest 200 posts of each of the post's 8 rarest tags, through the GIN index;
- up to 400 look-alikes whose hash has a 16-bit chunk within one bit of the post's, through four expression indexes (migration 0015). By the pigeonhole rule, that catches every hash within 7 bits.

Only those candidates are scored, and only the top ones get full rows.

## Media storage

```text
/data/original/ab/cd/<sha256>.<ext>
/data/thumb/ab/cd/<sha256>.jpg
/data/sample/ab/cd/<sha256>.jpg
/data/tmp/            (staging; writes are atomic renames)
```

Uploads are sniffed by content, not by name. Accepted formats:

| Formats | Kind |
|---|---|
| JPEG, PNG, WebP, BMP, TIFF, AVIF, JPEG XL | images |
| GIF | animation |
| MP4, WebM, MKV, MOV | video |

Thumbnails are JPEGs at most 512px on the long side. The `image` crate decodes the common formats. For video, AVIF and JPEG XL the server asks ffmpeg for a frame (10% into a video, at most 5 s) and ffprobe for the size and length (`posts.duration_ms`). Without ffmpeg those posts have no thumbnail and the client shows a placeholder. Images are decoded with their EXIF orientation applied, so thumbnails, sizes and edits match what a browser shows. At each start a background task retries posts that are still missing one. The gallery plays a muted preview of a video while the mouse is over it.

Originals are kept byte for byte. Big still images also get a **sample**, a smaller copy that the viewer and share pages show instead. The rules:
- The original is a JPEG, PNG, WebP, BMP or TIFF of at least 1 MiB, with no transparency.
- The copy is a JPEG at quality 85, at most 2560 px on the long side, with the original's colour profile.
- It is kept only when it is at most 75% of the original's size. Already compact JPEGs have none.

`posts.has_sample` is null until a post has been looked at. A background task at each start makes samples for older posts. `STARBOORU_SAMPLES=false` stops making them. The viewer swaps in the original when you zoom in or tap *Show original*, or always with Settings › *Always open originals*. Downloads and the editor use the original.

**Text in images (OCR).** `posts.ocr_at` is null until a still image has been read by tesseract (`ocr.rs`); `posts.ocr_text` holds what it found, or null. A GIN index on `to_tsvector('simple', ocr_text)` for live posts serves `text:` search. Uploads are read in the background, so OCR never delays or fails one; a task at each start reads older posts one at a time, and `POST /posts/{id}/ocr` reads one again. With an AI provider set up, the viewer's *Translate* sends that text (never the image) to it and shows the answer in the app's language, without storing it. For comics and manga, `panels.rs` sends the image itself and keeps the result in `post_translations` (post, language, JSONB blocks with boxes on a 0–1000 scale, model); the viewer draws the blocks over the image with the same zoom transform.

Media URLs look like `/media/thumb/<sha>.jpg?exp=…&sig=…`:
- The signature is an HMAC-SHA256 keyed by `STARBOORU_SECRET`.
- The expiry is rounded to 6-hour buckets, so URLs stay stable and browsers cache them.
- `<img>` and `<video>` tags work without sending the bearer token.

## Clients

| File | Role |
|---|---|
| `zoom.rs` | Zoom and pan maths for the viewer (scale around a point, clamped so the media keeps covering the stage). Unit tested. |
| `i18n.rs` | UI text. `t!("id")` / `t!("id", count = n)` looks a message up in the Fluent catalogue for the current language (`LANG`: the one picked in Settings, else the system locale, else English) and falls back to English, then to the id. Tests keep the catalogues in step with each other and with the source. |
| `locales/*.ftl` | One Fluent catalogue per language, compiled in with `include_str!`. `en.ftl` is the reference. |
| `oidc.rs` | Provider sign-in. Web: the page goes to the provider and comes back to the `/oidc` route, with the handle kept in `sessionStorage`. Desktop and Android: the system browser opens the provider (`webbrowser`), and a one-off listener on `127.0.0.1` receives the result. |
| `activity.rs` | Work that outlives a page, in global signals at the app's root. The upload queue holds files and addresses until *Upload*; then files go to a runner spawned on the root scope (one at a time, so leaving the page doesn't stop it, though closing the app does; the web app asks before closing with uploads left), and addresses go to the server as `POST /imports` jobs. A poller (mounted by the shell) keeps the job list current: every 2 s while jobs run, else every minute. The Upload item in the navigation shows how many uploads and imports are still going, and a notice sums them up when they finish. |
| `cache.rs` | Desktop and Android media cache. Thumbnails and the viewer's images load from `/sbcache/<variant>/<sha256>.<ext>?<signature>`, which a webview asset handler answers from `<cache dir>/media/` (on Android, the app's cache dir), or downloads from the signed-in server's `/media/…` (no token; 64 MiB at most), keeps and answers. Files are content-addressed, so they never go stale, though signed URLs change every 6 hours. Least recently used files go once the cache is over its limit (Settings › Storage on this device, 512 MB by default, off to 5 GB). The web app uses the browser cache. Key parsing and eviction are unit tested. |
| `download.rs` | *Download* buttons. The desktop app streams the signed URL into the Downloads folder (`Client::download_to`, through a `.part` file), never overwriting (`name (2).ext`), and reports the path in the shell's snackbar (`pages::notify`). The web app and Android use an `<a download>` link. File-name cleaning and the free-name rule are unit tested. |
| `power.rs` | Battery saver. It watches the Battery API and the data-saver hint where the webview has them, pauses videos when the app is hidden, and `power::active()` combines that with the setting. The root element carries `data-saver` so CSS can cut animations. The rule for combining the two is unit tested. |
| `system.rs` | What the device says about itself. On Android, through JNI (`robius-android-env`): the app's files and cache dirs, which `directories` can't find there, and on Android 12+ the wallpaper's accent (`system_accent1_500`). It is read at start and whenever the app comes back to the front, then turned into an OKLCH hue that seeds the palette while *Accent colour › Wallpaper* is chosen (the default). The hue conversion and the choice are unit tested. On Linux desktop it also decides whether Dioxus's XWayland and no-DMA-BUF fallback is needed: only for NVIDIA's driver (`development.md` › *Run the clients*). |
| `store.rs` | The session (server URL, token, user) and preferences (theme, accent hue, language, hand, battery saver, originals, media cache limit) in global signals. They persist to `localStorage` on the web, to `<config dir>/StarBooru/state.json` on desktop and to the app's private files dir on Android. The app's root element carries `data-theme`, `data-hand` and `data-saver` for the CSS. |
| `pages/` | `shell` (top bar, search with tag autocomplete, navigation; bottom bar on phones; a snackbar for notices that arrive after the click), `login`, `gallery` (infinite scroll), `post` (viewer with zoom, pan, previous/next and fullscreen; votes, edit, delete/restore; it fetches the neighbouring posts ahead, sizes the stage from the post's width and height and shows the thumbnail until the image loads; the gallery hands over the clicked post and grows its thumbnail into the stage with a view transition, except on Linux desktop, where WebKitGTK crashes in one), `editor` (the sheet that turns, flips and crops an image or trims a video; the crop maths is unit tested), `upload` (drag and drop, the queue and the server's import jobs), `boorus` (search another booru through the server, pick results and queue them as server imports with their tags, rating and source page; the sites searched and their accounts are kept in `Prefs`), `trash` (recycle bin), `tags` (tag list; tag page with category, aliases and implications), `explore`, `settings`, `help` (search cheat sheet), `legal` (Privacy Policy and Terms of Service, outside the signed-in shell), `share` (the sheet that makes a link, Settings › Shared links, and the public `/s/{token}` page outside the shell) |
| `assets/main.css` | The design system. Material-You-style colour roles come from one OKLCH seed hue (`--seed-hue`), with light, dark and system themes. Glass applies to the bars only. Animations use spring easing and are disabled under reduced motion. |
