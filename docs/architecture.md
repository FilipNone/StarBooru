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
| `crates/core` (`starbooru-core`) | API models (`Post`, `Tag`, `User`, …), tag normalisation, the search query parser. Pure Rust, no I/O. |
| `crates/client` (`starbooru-client`) | `Client`: one async method per endpoint, with a bearer token. Maps HTTP errors to `ClientError` (including `RateLimited { retry_after }`). Runs on wasm and native. |
| `apps/server` (`starbooru-server`) | axum router, auth, posts, tags, storage, thumbnails, signed media. |
| `apps/app` (`starbooru-app`) | Dioxus 0.7 UI. Feature `web`, `desktop` or `mobile` selects the renderer. |

### Server modules

| Module | What it does |
|---|---|
| `config.rs` | Environment variables (see `deployment.md`) |
| `routes/` | One file per resource: `auth`, `posts`, `tags`, `trash`. `mod.rs` builds the router and its layers (CORS, compression, tracing, panic catcher, SPA fallback). |
| `auth.rs` | Bearer-token extractor; Argon2id password hashing (on the blocking pool) |
| `storage.rs` | Format sniffing, content-addressed storage, thumbnails |
| `media.rs` | HMAC-signed media URLs and the file handler (supports Range for video seeking) |
| `error.rs` | `AppError` → JSON `{code, message}` with the right status |
| `testing.rs` | Test-only harness: a fresh database per test and the real router |

## Data model

PostgreSQL schema: `apps/server/migrations/`.

| Table | Contents |
|---|---|
| `users` | Usernames are case-insensitively unique. Roles are `admin` and `user`; the first account becomes admin. |
| `sessions` | The SHA-256 of each bearer token (`sb_<64 hex>`) and its expiry, 90 days. The plain token is never stored. |
| `tags` | Normalised name, category (general / artist / character / copyright / meta) and a `post_count` kept in step inside the same transaction as post edits |
| `posts` | One per file: `sha256` (unique, so duplicates are refused), format, size, dimensions, rating, source, score, `tag_ids BIGINT[]`, `deleted_at` (soft delete) |
| `post_votes` | One vote per user and post, −1 or +1. `posts.score` is their sum. |
| `tag_aliases` | `alias → tag_id`. Aliases are resolved when tagging, searching and in autocomplete. |
| `tag_implications` | `tag_id → implied_id`. Expanded transitively (a recursive CTE) when tags are set, so implied tags are stored on posts. Cycles are refused. Edits take an advisory lock, since they may rewrite many posts. |

### Search

Tags are stored on each post as a `BIGINT[]` with a GIN index. A query like `blue_sky night -cloud rating:g order:score` becomes:

1. The tag names are resolved to ids, following aliases. If an included tag doesn't exist, the result is empty, with no scan.
2. The SQL is `tag_ids @> {include} AND NOT tag_ids && {exclude} AND rating = ANY(...)`.
3. Rows are ordered by id, score or random, and paginated by limit and offset (at most 200 per page).

Partial indexes cover live posts, so the recycle bin doesn't slow normal browsing. The query syntax is in `api.md`.

## Media storage

```text
/data/original/ab/cd/<sha256>.<ext>
/data/thumb/ab/cd/<sha256>.jpg
/data/tmp/            (staging; writes are atomic renames)
```

Uploads are sniffed by content, not by name. Accepted formats:

| Formats | Kind |
|---|---|
| JPEG, PNG, WebP, BMP, TIFF, AVIF, JPEG XL | images |
| GIF | animation |
| MP4, WebM, MKV, MOV | video |

Thumbnails are 512px JPEGs and are made for the formats the `image` crate decodes. Videos, AVIF and JPEG XL have no thumbnail yet; the client shows a placeholder.

Media URLs look like `/media/thumb/<sha>.jpg?exp=…&sig=…`:
- The signature is an HMAC-SHA256 keyed by `STARBOORU_SECRET`.
- The expiry is rounded to 6-hour buckets, so URLs stay stable and browsers cache them.
- `<img>` and `<video>` tags work without sending the bearer token.

## Clients

| File | Role |
|---|---|
| `store.rs` | The session (server URL, token, user) and preferences (theme, accent hue) in global signals. They persist to `localStorage` on the web and to `<config dir>/StarBooru/state.json` on native. |
| `pages/` | `shell` (top bar, search with tag autocomplete, navigation; bottom bar on phones), `login`, `gallery` (infinite scroll), `post` (viewer, votes, edit, delete/restore), `upload` (drag and drop, queue), `trash` (recycle bin), `tags` (tag list; tag page with category, aliases and implications), `settings` |
| `assets/main.css` | The design system. Material-You-style colour roles come from one OKLCH seed hue (`--seed-hue`), with light, dark and system themes. Glass applies to the bars only. Animations use spring easing and are disabled under reduced motion. |
