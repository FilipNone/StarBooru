# HTTP API (v1)

All endpoints are under `/api/v1`. Bodies are JSON unless noted. The Rust types are in `crates/core/src/model.rs`, and `starbooru-client` wraps every call.

**Auth.** Send `Authorization: Bearer <token>`. Every endpoint except `info`, `auth/register` and `auth/login` requires it. The token is a session token from sign-in (`sb_…`) or an [API key](#api-keys) (`sbk_…`).

**Errors.** Errors are returned as `{"code": "...", "message": "..."}` with a matching status:

| Status | Meaning |
|---|---|
| 400 | bad input |
| 401 | missing or expired token |
| 403 | not allowed (`forbidden`, or `quota_exceeded` for an upload past the account's storage limit) |
| 404 | not found |
| 409 | conflict (`duplicate`, `username_taken`, `deleted`, `not_deleted`, `implication_cycle`, `name_taken`) |
| 413 | upload too large |
| 415 | unsupported file type |
| 429 | rate limited; `starbooru-client` waits out `Retry-After` (up to 30 s) and retries twice |
| 500 | internal error (details only in the server log) |
| 502 | `fetch_failed`: URL import could not download the address |

## Server

| Method | Path | |
|---|---|---|
| GET | `/info` | `ServerInfo`: `name`, `version`, `has_users`, `registration_open`, `max_upload_bytes`, `url_import` (whether `POST /posts/import` is on), `oidc` (the identity provider's name for a "Sign in with …" button, `null` when OpenID Connect sign-in is off) |
| GET | `/healthz` (not under `/api/v1`) | `ok` when the database answers |

`name` and `registration_open` follow the admin settings (see [Administration](#administration)).

## Accounts

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/auth/register` | `{username, password}` | `AuthResponse {token, user}`. The first account becomes admin. Later accounts need `STARBOORU_ALLOW_REGISTRATION`. 429 after 10 attempts from one address in an hour. |
| POST | `/auth/login` | `{username, password}` | `AuthResponse`. 401 for a wrong username or password. 429 `rate_limited` with `Retry-After` once an account has 10 failures in 15 minutes (the right password waits too) or an address has 50. A successful sign-in resets the account's count. |
| POST | `/auth/logout` | | 204; revokes this token |
| GET | `/auth/me` | | `User` |
| GET | `/auth/usage` | | `Usage {posts, bytes, quota_bytes}`: what the account uploaded (recycle bin included) and its storage limit (`null` for none) |
| PUT | `/auth/password` | `PasswordChange {current, new}` | 204. Signs out every other session of the account. 400 if `current` is wrong or `new` breaks the password rules. 429 after 10 wrong `current` passwords in 15 minutes. An account made by a provider sign-in has no password until it sets one; `current` is then ignored. |
| GET | `/auth/sessions` | | `SessionInfo[] {id, client, created_at, last_used_at, expires_at, current}`: the account's live sessions, this one first. `client` is a short description such as "Firefox on Linux", derived from the User-Agent at sign-in (the header itself is not stored). `last_used_at` is updated at most every 5 minutes. |
| DELETE | `/auth/sessions/{id}` | | 204; signs that session out. 404 if it isn't one of yours. |
| DELETE | `/auth/sessions` | | `RevokeResult {revoked}`; signs out every session but this one |

Account rules:
- Usernames are 2 to 32 characters of `A–Z a–z 0–9 _ . -` and are unique regardless of case.
- Passwords are 8 characters or more.
- Tokens last 90 days.
- With an API key, the `/auth/logout`, `/auth/password` and `/auth/sessions` endpoints return 403 `forbidden`.
- A disabled account gets 403 `forbidden` at sign-in (after the right password), and its sessions and keys answer 401.

### Provider sign-in (OpenID Connect)

On when the server has `STARBOORU_OIDC_*` set (`/info` then names the provider in `oidc`); otherwise these endpoints answer 404.

1. `POST /auth/oidc/begin` with `OidcBegin {return_to}` answers `OidcStart {url, handle}`. Keep `handle` secret and open `url` in a browser.
2. The provider sends the browser back to `GET /auth/oidc/callback` on the server, which sends it on to `return_to` with `oidc_code=<code>` added to the query, or `oidc_error=denied` (the user said no) or `oidc_error=failed`.
3. `POST /auth/oidc/finish` with `OidcFinish {code, handle}` answers `AuthResponse`, like `/auth/login`.

`return_to` must be a page of the server's own web app (under `STARBOORU_PUBLIC_URL`) or a loopback address (`http://127.0.0.1:<port>/…`, `http://[::1]…`, `http://localhost…`) a native app listens on; anything else is 400. A sign-in must be finished within 10 minutes, once. A code with the wrong handle, used twice or too late is 400.

At `finish`, a provider account seen before signs in to the account it is linked to. A new one makes an account, if registration is open or there are no accounts yet (the first becomes admin); its username comes from the provider's `preferred_username` or email, made unique with a number. With registration closed it is 403 `forbidden`, as for a disabled account.

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/auth/oidc/begin` | `OidcBegin {return_to}` | `OidcStart {url, handle}`. 429 when too many sign-ins are pending. |
| POST | `/auth/oidc/begin-link` | `OidcBegin` | `OidcStart`, to link a provider account to the signed-in one |
| GET | `/auth/oidc/callback` | | 303 to `return_to`. An unknown or expired sign-in gets a short HTML page (400) instead. |
| POST | `/auth/oidc/finish` | `OidcFinish {code, handle}` | `AuthResponse` |
| POST | `/auth/oidc/link` | `OidcFinish` | `Identity`. Only the account that called `begin-link` can finish it. 409 `identity_taken` if that provider account is linked already. |
| GET | `/auth/identities` | | `SignInMethods {password, identities: Identity[] {id, label, created_at, last_used_at}}`: whether the account has a password, and its linked provider accounts. `label` is the provider's username or email. |
| DELETE | `/auth/identities/{id}` | | 204. 400 if it is the account's last way to sign in (no password, no other identity). 404 if it isn't yours. |

The `begin-link`, `link` and `identities` endpoints need a session; API keys get 403.

## API keys

Long-lived tokens for scripts and other tools. A key acts as the user who made it, with these limits:
- It can't manage sign-ins, the password or keys. Those endpoints answer 403 `forbidden`, so a leaked key can't take over the account.
- A read-only key answers 403 `forbidden` ("this API key is read-only") to anything but `GET` and `HEAD`.

Managing keys needs a sign-in session. Changing the password keeps the account's keys; remove any you no longer trust.

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/keys` | | `ApiKey[] {id, name, prefix, read_only, created_at, last_used_at, expires_at}`, newest first. `prefix` is the first 12 characters of the token. `last_used_at` is `null` until first use and is updated at most every 5 minutes. `expires_at` is `null` for keys that don't expire. Expired keys stay listed until removed. |
| POST | `/keys` | `NewApiKey {name, read_only?, expires_days?}` | `CreatedApiKey {key, token}`. The token is shown only here; the server keeps only its SHA-256. The name is 1–64 characters with whitespace collapsed. `expires_days` is 1–3650, or omitted for never. 400 past 50 keys. |
| DELETE | `/keys/{id}` | | 204; the key stops working at once. 404 if it isn't the caller's. |

## Administration

Admins only; others get 403. API keys may read these but not change anything.

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/admin/stats` | | `ServerStats`: `version`, `started_at`, `users`, `disabled_users`, live `images`/`animations`/`videos`, `bin_posts`, `tags`, `media_bytes` (originals, bin included), `bin_bytes`, `database_bytes`, `uploads_by_day` (`[{day, posts}]`, the last 30 UTC days oldest first), `max_upload_bytes`, `video_thumbnails`, `url_import` |
| GET | `/admin/settings` | | `ServerSettings {name, allow_registration, trash_days}`, the values in force |
| PUT | `/admin/settings` | `ServerSettings` | The saved settings. They apply at once. Each starts from its environment variable (`STARBOORU_NAME`, `STARBOORU_ALLOW_REGISTRATION`, `STARBOORU_TRASH_DAYS`), and a value equal to the variable's keeps following it. `name` is 1–64 characters with whitespace collapsed; `trash_days` is 0–3650. |
| GET | `/admin/users` | | `AdminUser[] {id, username, role, created_at, disabled, posts, bytes, quota_bytes, last_active}`, oldest first. `posts` and `bytes` count uploads, recycle bin included. `last_active` is the latest use of any session or key. |
| PATCH | `/admin/users/{id}` | `UserUpdate {role?, disabled?, quota_bytes?}` | The updated `AdminUser`. Missing fields stay as they are; `"quota_bytes": null` removes the limit. Disabling signs the account out everywhere. 400 for your own account (so an admin always remains), 404 for an unknown id. |
| POST | `/admin/users/{id}/password` | `PasswordReset {new}` | 204. Signs the account out everywhere; its API keys keep working. 400 for your own account (use `PUT /auth/password`). |
| DELETE | `/admin/users/{id}` | | 204. Removes the account with its sessions, keys, votes (taken off the scores) and saved searches. Its posts stay, without an uploader. 400 for your own account. |

An upload or URL import that would take the uploader past `quota_bytes` answers 403 `quota_exceeded`.

### Backups

Database backups (`pg_dump` archives, custom format). They hold every account's password hash, so these need an admin signed in with a session: API keys get 403 even for `GET`.

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/admin/backups` | | `Backups {available, running, schedule, items}`. `available` is false when `pg_dump` is missing or turned off. `schedule` is `BackupSchedule {every_days, keep}`. `items` is `Backup[] {name, bytes, created_at, automatic, url}`, newest first; `automatic` marks the schedule's backups. |
| POST | `/admin/backups` | | The new `Backup`, once written (this can take a while). 403 when backups are unavailable, 409 `backup_running` while another one is being made. |
| PUT | `/admin/backups/schedule` | `BackupSchedule` | The saved schedule. `every_days` is 0–365 (0 turns automatic backups off); `keep` is 1–100. Each starts from its variable (`STARBOORU_BACKUP_DAYS`, `STARBOORU_BACKUP_KEEP`). Automatic backups beyond `keep` are removed at once; ones made by hand stay. |
| DELETE | `/admin/backups/{name}` | | 204; 404 for an unknown name. |

A `Backup`'s `url` is a signed path relative to the server, valid for an hour. Fetch it without the bearer token; it answers with the file as an attachment.

## Posts

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/posts?q=&page=&limit=` | | `PostPage {posts, page, limit, has_more}`. `page` starts at 1 (at most 10000); `limit` is at most 200, default 60. |
| POST | `/posts` | multipart: `file` (required), `tags` (space-separated), `rating` (`g`/`s`/`q`/`e`), `source` | `Post`, or 409 `duplicate` if the same file exists |
| POST | `/posts/import` | `ImportRequest {url, tags?, rating?, source?}`. `tags` is a list; `source` defaults to `url`. | `Post`, like an upload. 400 for a non-http(s) address, credentials in the URL, a private-network address (unless the server allows them) or import turned off; 413 over the upload limit; 409 `duplicate`; 502 `fetch_failed` when the remote server errors, times out or redirects more than 5 times. |
| GET | `/posts/{id}` | | `Post` |
| GET | `/posts/{id}/neighbors?q=` | | `Neighbors {prev, next}`: the ids just before and after the post in the results of search `q` (same filters and order), or null at either end. Both are null for `order:random` or when `q` matches nothing. |
| PATCH | `/posts/{id}` | `PostUpdate {tags?, rating?, source?}` | `Post`. `tags` replaces the whole list. |
| DELETE | `/posts/{id}` | | 204; moves the post to the recycle bin (admin or uploader) |
| POST | `/posts/{id}/restore` | | `Post` |
| POST | `/posts/{id}/purge` | | 204; permanently deletes a post that is in the recycle bin, with its files (admin or uploader). 409 `not_deleted` if the post is live. |
| POST | `/posts/bulk` | `BulkEdit {ids, add_tags?, remove_tags?, rating?, delete?}` | `BulkResult {updated, skipped}`. Up to 500 distinct ids. Edits run in one transaction: tags in `remove_tags` (aliases resolve) come off, then `add_tags` go on with their implications. With `delete: true` (and no edits) each post moves to the recycle bin; posts you may not delete are skipped. Missing posts, and posts in the recycle bin when editing, are skipped. 400 for no ids, too many, an invalid tag, a post over the tag limit, no change, or delete combined with edits. |
| PUT | `/posts/{id}/vote` | `{value: -1 \| 0 \| 1}` | `Post` with the new `score` and `my_vote` |

A `Post` carries `file_url` and `thumb_url`. These are signed paths relative to the server, valid for at least 6 hours. Fetch them without the bearer token. `thumb_url` is null when no thumbnail exists. `duration_ms` is the length of a video, or null.

### Search syntax (`q`)

Terms are separated by spaces, with at most 40 terms.

| Term | Meaning |
|---|---|
| `blue_sky` | must have the tag |
| `-cloud` | must not have the tag |
| `rating:g` / `-rating:e` | rating is (not) general, sensitive, questionable or explicit (`g s q e`); repeat to allow several, e.g. `rating:g rating:s` |
| `type:image` / `type:animation` / `type:video` | media kind |
| `order:newest` (default), `order:oldest`, `order:score`, `order:size`, `order:pixels`, `order:tags`, `order:updated`, `order:random` | sort (`sort:` works too). Add `_asc` or `_desc` to set the direction, e.g. `order:size_asc` for the smallest files first. `pixels` is width × height; `tags` is the tag count; `updated` is the last edit. |
| `status:deleted` | browse the recycle bin |

Tags are normalised: trimmed, lower-cased, spaces and underscores folded to one `_`, and at most 128 characters.

## Recycle bin

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/trash/empty?older_than_days=` | | `PurgeResult {purged}`. Admin only. Purges every post in the bin, or only those deleted at least `older_than_days` ago. |

Browse the bin with `GET /posts?q=status:deleted`. `STARBOORU_TRASH_DAYS` purges old posts automatically.

## Duplicates

Finds posts that look alike even when the files differ (resized, re-encoded, lightly edited). Each post with a thumbnail has a fingerprint: a 64-bit difference hash of its brightness and its average colour. Two posts match when their hashes differ in at most `distance` bits and each colour channel is within 24 of the other's. Groups are transitive: A and C are grouped when both match B.

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/duplicates?distance=&limit=` | | `Duplicates {distance, total, unhashed, groups}`. `distance` is 0–10 (default 6). `limit` is how many groups to return, 1–100 (default 30). `total` counts every group; `unhashed` counts posts that can't be compared yet (no thumbnail, or not fingerprinted yet). Each `DuplicateGroup {distance, size, posts}` gives the largest distance linking it, its size, and up to 50 posts, best first (most pixels, then largest file). Closest groups come first. Pairs marked "not duplicates" are never linked. |
| POST | `/duplicates/resolve` | `ResolveDuplicates {keep, remove, merge_tags}` | `Post` (the kept one). Moves `remove` to the recycle bin in one transaction. With `merge_tags` the kept post also gets their tags, and their source if it has none. 404 if any post is missing or already in the bin; 403 unless the caller may delete each removed post. 400 if `keep` is in `remove` or `remove` is empty or over 50. |
| POST | `/duplicates/dismiss` | `DismissDuplicates {ids}` | 204. Marks every pair among 2–50 posts as not duplicates. |

Fingerprints for posts uploaded before this feature are computed in the background at startup.

## Tags

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/tags?q=&order=&page=&limit=` | | `Tag[]`. `order` is `count` (most used first, the default) or `name`. Without `q`, only tags in use are listed. With `q` it is the autocomplete list: tags whose name or alias starts with `q`. `limit` is 1–100 (default 20). |
| GET | `/tags/{name}` | | `TagDetail {tag, aliases, implies, implied_by}`. An alias resolves to its tag. 404 if neither exists. |
| PATCH | `/tags/{name}` | `{category}` | `Tag`. Categories: `general`, `artist`, `character`, `copyright`, `meta`. Any signed-in user. |
| POST | `/tags/{name}/aliases` | `{alias}` | `TagDetail`. Admin only. If a tag called `alias` exists, it is merged in: its posts, aliases and implications move to `{name}` and it is deleted. 400 if `alias` is the tag itself. |
| DELETE | `/tags/{name}/aliases/{alias}` | | `TagDetail`. Admin only. |
| POST | `/tags/{name}/implications` | `{implies}` | `TagDetail`. Admin only. Missing tags are created. Posts that already have `{name}` get the implied tags (transitively). 409 `implication_cycle` if `implies` already implies `{name}`; 400 if they are the same tag. |
| DELETE | `/tags/{name}/implications/{implied}` | | `TagDetail`. Admin only. Posts keep the tags they already got. |

Tags are resolved the same way everywhere: an alias in a post's tags or in a search means its tag, and implied tags are added whenever tags are set on a post. Implied tags are stored on the post, so they show up in its tag list and in searches. Path segments are percent-encoded.

## Explore

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/explore?days=&q=` | | `Explore {days, trending, tags, on_this_day, random}`. `days` is the window, 1 to 365 (default 7). `trending`: up to 24 posts with the most net upvotes cast or changed in the window (only those above zero). `tags`: up to 20 `TagTrend {tag, recent}`, the tags most used on posts added in the window, without the tags `q` requires. `on_this_day`: up to 12 posts added on today's month and day (UTC) in earlier years, newest first. `random`: 12 random posts. `q` uses the search syntax and narrows every section; its `order:` and `status:` are ignored. 400 for a bad query. |

## Saved searches

Each user's own list; other users never see it.

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/searches` | | `SavedSearch[] {id, name, query, created_at}`, by name |
| POST | `/searches` | `SaveSearchRequest {name, query}` | `SavedSearch`. Whitespace is collapsed. The name is 1–64 characters; the query must parse and is at most 1000 characters. 409 `name_taken` if the user already has that name (case-insensitive); 400 past 200 saved searches. |
| DELETE | `/searches/{id}` | | 204. 404 if it isn't the caller's. |

## Share links

A share link shows chosen posts to anyone who has it, without an account. It shows only the media: no tags, votes, ids or account details. Any signed-in user can share any post they can see.

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/shares` | | `Share[] {id, token, title, posts, cover_url, created_at, expires_at, views, last_viewed_at}`, the caller's links, newest first. `posts` counts the posts the link was made with. `cover_url` is the signed thumbnail of the first post still shown, or `null`. `views` counts opens of the public view. Expired links stay listed until removed. |
| POST | `/shares` | `NewShare {post_ids, title?, days?}` | `Share`. `post_ids` keeps its order, repeats are dropped, 1–500 posts. The title is trimmed, at most 100 characters, and empty means none. `days` is 1–365, or omitted to keep the link until it is removed. 400 if a post doesn't exist or is in the recycle bin, and past 500 links. |
| DELETE | `/shares/{id}` | | 204; the link stops working at once. 404 if it isn't the caller's. Media URLs already handed out stay valid until their signature expires (up to 12 hours). |
| GET | `/shared/{token}` | | **No auth.** `SharedView {title, server, expires_at, posts}`, where `posts` are `SharedPost {kind, mime, width, height, duration_ms, file_url, thumb_url}` in the link's order. Posts moved to the recycle bin since are left out. 404 for a malformed, unknown, expired or removed link, or one whose owner is disabled. Answers with `Cache-Control: no-store` and `Referrer-Policy: no-referrer`. |

The page to give out is `/s/{token}` on the server, which the web app serves (`starbooru_core::share::share_path`).

## Media

```text
GET /media/{original|thumb}/{sha256}.{ext}?exp=<unix>&sig=<hex>
```

Media URLs come from the API, so never build one by hand. Invalid or expired signatures get a 403. Range requests are supported. Responses are cacheable (`private, max-age=21600, immutable`).
