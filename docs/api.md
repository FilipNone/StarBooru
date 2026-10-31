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
| GET | `/info` | `ServerInfo`: `name`, `version`, `has_users`, `registration_open`, `max_upload_bytes`, `url_import` (whether `POST /posts/import` is on), `oidc` (the identity provider's name for a "Sign in with …" button, `null` when OpenID Connect sign-in is off), `video_editing` (whether `POST /posts/{id}/edit` can trim videos: the server has ffmpeg) |
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
| GET | `/admin/settings` | | `ServerSettings {name, allow_registration, trash_days, auto_meta_tags}`, the values in force |
| PUT | `/admin/settings` | `ServerSettings` | The saved settings. They apply at once. Each starts from its environment variable (`STARBOORU_NAME`, `STARBOORU_ALLOW_REGISTRATION`, `STARBOORU_TRASH_DAYS`, `STARBOORU_AUTO_TAGS`), and a value equal to the variable's keeps following it. `name` is 1–64 characters with whitespace collapsed; `trash_days` is 0–3650. `auto_meta_tags` (automatic meta tags such as `highres` and `video` on new posts) may be left out by older clients and then means `true`. |
| GET | `/admin/users` | | `AdminUser[] {id, username, role, created_at, disabled, posts, bytes, quota_bytes, last_active}`, oldest first. `posts` and `bytes` count uploads, recycle bin included. `last_active` is the latest use of any session or key. |
| PATCH | `/admin/users/{id}` | `UserUpdate {role?, disabled?, quota_bytes?}` | The updated `AdminUser`. Missing fields stay as they are; `"quota_bytes": null` removes the limit. Disabling signs the account out everywhere. 400 for your own account (so an admin always remains), 404 for an unknown id. |
| POST | `/admin/users/{id}/password` | `PasswordReset {new}` | 204. Signs the account out everywhere; its API keys keep working. 400 for your own account (use `PUT /auth/password`). |
| DELETE | `/admin/users/{id}` | | 204. Removes the account with its sessions, keys, votes (taken off the scores) and saved searches. Its posts stay, without an uploader. 400 for your own account. |
| GET | `/admin/ai` | | `AiAdminStatus {hardware, runtime {ready, reason, gpu}, autotag, auto_translate, tools}`: per tool (`tagging`, `translation`, `comics`) its `settings {mode, preset, model, api_url, api_model, api_key_set}`, `presets [{preset, available, model, est_ms}]`, `models [{id, name, size, ram_mb, high_accuracy, est_ms, measured, gpu, installed, fits, progress {done, total}, error}]`, `active`, `state` (`off`, `ready`, `installing`, `failed`) and `error`. Keys are never returned. |
| PUT | `/admin/ai` | `AiGeneralUpdate {autotag, auto_translate}` | `AiAdminStatus`. `auto_translate` is a language code, or `""` for off, that new images with OCR text are translated into; 400 when comics are off. |
| PUT | `/admin/ai/tools/{tool}` | `AiToolUpdate {mode, preset, model, api_url, api_model, api_key?}` | `AiAdminStatus`. `mode` is `off`, `local` or `api`; `preset` is `fast`, `balanced` or `accurate`; an empty `model` follows the preset. API mode needs `api_url` and `api_model`. `api_key` missing keeps the stored key, `""` clears it. The models it needs start downloading. 400 for invalid values or another tool's model. |
| POST | `/admin/ai/tools/{tool}/test` | | `AiTestResult {ms}`: a short text chat with the tool's saved API. 400 outside API mode; 502 `fetch_failed` when it errors. |
| POST | `/admin/ai/models/{id}/install` | | `AiAdminStatus`. Queues the download; poll `GET /admin/ai` for `progress` and `error`. |
| DELETE | `/admin/ai/models/{id}` | | `AiAdminStatus`. 400 while a tool uses the model. Of its files, removes those the fewest other installed models need, so models sharing a file with it can stop being installed. |

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
| GET | `/posts?q=&page=&limit=` | | `PostPage {posts, page, limit, has_more, corrected?}`. `page` starts at 1 (at most 10000); `limit` is at most 200, default 60. `corrected` lists `TagCorrection {from, to}` for the unknown tags the search corrected; it is left out when there are none. |
| POST | `/posts` | multipart: `file` (required), `tags` (space-separated), `rating` (`g`/`s`/`q`/`e`), `source` | `Post`, or 409 `duplicate` if the same file exists |
| POST | `/posts/import` | `ImportRequest {url, tags?, rating?, source?}`. `tags` is a list; `source` defaults to `url`. | `Post`, like an upload. 400 for a non-http(s) address, credentials in the URL, a private-network address (unless the server allows them) or import turned off; 413 over the upload limit; 409 `duplicate`; 502 `fetch_failed` when the remote server errors, times out or redirects more than 5 times. |
| GET | `/posts/{id}` | | `Post`, with `ocr_text` and `tag_details: [PostTag {name, category, section}]`. `section` is the top of the tag's parent chain (`clothing` for `boots`), the tag itself when it heads a tree, or null for a tag on its own; the viewer groups general tags by it. Also `figures`, the people in the picture (see [People](#people-in-a-post)), left out when there are none. The answers of `PATCH /posts/{id}`, the vote, restore and OCR rescan carry the same fields; lists leave them out. |
| GET | `/posts/{id}/neighbors?q=` | | `Neighbors {prev, next}`: the ids just before and after the post in the results of search `q` (same filters and order), or null at either end. Both are null for `order:random` or when `q` matches nothing. |
| GET | `/posts/{id}/related?limit=` | | `RelatedPosts {posts}`: up to `limit` live posts like this one (default 12, at most 48), most alike first. Shared tags count by rarity, so a tag on a handful of posts outweighs one on half the collection; posts whose look (difference hash) is close count too, even without tags. Candidates come from the post's 8 rarest tags and from look-alikes, so the list is a suggestion, not an exhaustive search. Empty when nothing is alike. |
| PATCH | `/posts/{id}` | `PostUpdate {tags?, rating?, source?}` | `Post`. `tags` replaces the whole list. |
| DELETE | `/posts/{id}` | | 204; moves the post to the recycle bin (admin or uploader) |
| POST | `/posts/{id}/restore` | | `Post` |
| POST | `/posts/{id}/ocr` | | `Post` with `ocr_text`: reads the text in the image again (tesseract on the server; it can take several seconds). 400 when OCR is off on the server. |
| POST | `/posts/{id}/purge` | | 204; permanently deletes a post that is in the recycle bin, with its files (admin or uploader). 409 `not_deleted` if the post is live. |
| POST | `/posts/bulk` | `BulkEdit {ids, add_tags?, remove_tags?, rating?, delete?}` | `BulkResult {updated, skipped}`. Up to 500 distinct ids. Edits run in one transaction: tags in `remove_tags` (aliases resolve) come off, then `add_tags` go on with their implications. With `delete: true` (and no edits) each post moves to the recycle bin; posts you may not delete are skipped. Missing posts, and posts in the recycle bin when editing, are skipped. 400 for no ids, too many, an invalid tag, a post over the tag limit, no change, or delete combined with edits. |
| POST | `/posts/{id}/edit` | `MediaEdit {rotate?, flip_h?, flip_v?, crop?, trim?, bin_original?}` | `Post`: a new post with the edited file and the original's tags, rating and source. See [Editing media](#editing-media). |
| GET | `/ai` | | `AiStatus {enabled, model, tagging, translation, comics}`: which AI tools are on, so clients know whether to offer *Suggest tags* (`tagging`), *Translate* (`translation`) and the translate button on images (`comics`). `model` is the tagging model. |
| POST | `/posts/{id}/suggest-tags` | | `TagSuggestions {tags: [{name, category, exists}], rating, figures}`: up to 40 booru-style tags, the people with their tags by slot (`figures`, left out when there are none; names as the model gave them) and a rating an AI vision model suggests from the post's thumbnail. Names are normalised and resolved through aliases; `exists` says whether the tag is already on the server, and `category` is the existing tag's, or the one the model gave. Nothing on the post changes. 400 when AI is off or the post has no thumbnail; 502 `fetch_failed` when the provider errors (`AI provider answered 401`), times out or answers something unreadable. |
| POST | `/posts/{id}/translate` | `TranslateRequest {to}` | `Translation {text}`: the post's `ocr_text` translated by the AI provider into `to`, a BCP 47 code such as `en` or `pt-BR` (the clients send their UI language). Only the text is sent, not the image, and nothing is stored. At most 10,000 characters go each way. 400 for a bad code, when AI is off or when the post has no text; 404 for a missing or binned post; 502 `fetch_failed` as for suggestions, or for an empty answer. |
| GET | `/posts/{id}/translations` | | `[PanelTranslation {lang, blocks: [TextBlock {x, y, w, h, text, translation}], model, created_at}]`: the kept translations drawn over the image, newest first. Box edges are on a 0–1000 scale of the image's width and height. 404 for a missing or binned post. |
| POST | `/posts/{id}/translations` | `TranslateRequest {to}` | `PanelTranslation`: sends the still image (fit in 1536 px) to the translation model, which finds each piece of text, its box and its translation into `to`, for comic and manga pages. Kept per post and language, replacing an older one; an answer with no text (`blocks: []`) is returned but not kept. 400 for a bad code, when translation is off or for a video, GIF or unreadable format; 404 for a missing or binned post; 502 as for suggestions, or when the answer has no block list. |
| PUT | `/posts/{id}/figures` | `FiguresUpdate {figures: [Figure]}` | `Post`: replaces the post's people and updates its tags to match. See [People](#people-in-a-post). 400 for more than 20 people, more than 64 tags on one, more tags than a post can hold (512) or an invalid tag; 404 for a missing post; 409 `deleted` for a post in the recycle bin. |
| PUT | `/posts/{id}/vote` | `{value: -1 \| 0 \| 1}` | `Post` with the new `score` and `my_vote` |

A `Post` carries `file_url`, `thumb_url` and `sample_url`. These are signed paths relative to the server, valid for at least 6 hours. Fetch them without the bearer token. `thumb_url` is null when no thumbnail exists. `sample_url` is a smaller JPEG copy of a big image, meant for viewing; it is null for most posts, and `file_url` stays the original to download. `duration_ms` is the length of a video, or null. `file_date` is the date in the file, or null: EXIF for images (its wall clock, marked UTC, because EXIF has no time zone) and the creation time for videos. `palette` lists the picture's main colours as `#rrggbb`, most common first; it is left out when there are none. `GET /posts/{id}` (and `POST /posts/{id}/ocr`) also carry `ocr_text`, the text found in a still image by OCR, lines separated by `\n`; it is left out when there is none, when the image is not read yet, and in lists.

### People in a post

A `Figure` is one person (or creature) in the picture: `{kind, character?, cosplay?, tags}`. `kind` is `girl`, `boy` or `other`. `character` is who they are, a tag that becomes a character tag. `cosplay` is the character they are dressed as, without the suffix: the post gets `name_(cosplay)` (placed under `cosplay` in the tree when it has no parent) and `cosplay`. `tags` maps slots to tag lists. The slots are `head`, `body`, `bottom`, `shoes` and `wear` (other clothing) for clothing, and `shape`, `eyes`, `skin`, `hair`, `pose` and `other` for the body.

A figure's tags are on the post too. Saving figures adds their tags (aliases resolved, missing tags created, prefixes dropped) and removes the ones only the old figures gave. Taking a tag off the post with `PATCH /posts/{id}` or a bulk edit takes it off the figures too. While a post has figures, its count tags follow them: `1girl`…`5girls`, `6+girls` and `multiple_girls`, the same for `boy` and `other`, and `solo` for one figure. A count tag the figures don't call for is dropped, and the ones they do call for come back. A tag sits in one slot per figure: the first it is given. Count tags given in a slot are ignored. Automatic AI tagging of an upload also saves the people the model saw, when the post has none yet.

### Editing media

`POST /posts/{id}/edit` never changes a file: originals are immutable, so the result is a new post with the original's people. Anyone who can see the post may edit it; `bin_original: true` also moves the original to the recycle bin and needs the rights to delete it (403 otherwise, checked before anything is made).

- **Images** (JPEG, PNG, WebP, BMP, TIFF; not GIF, AVIF or JPEG XL): `rotate` is clockwise degrees (0, 90, 180, 270), then `flip_h`/`flip_v` mirror the turned image, then `crop {x, y, width, height}` cuts a rectangle in pixels of the turned and flipped image. The result keeps the format (JPEG at quality 92, WebP lossless) and drops metadata, with EXIF orientation applied.
- **Videos**: only `trim {start_ms, end_ms}`, inside the video and at least 100 ms long. The server first copies the streams, which is lossless but starts at a keyframe; when that lands more than half a second early it re-encodes the cut (H.264/AAC, or VP9/Opus for WebM). Needs ffmpeg (`/info` → `video_editing`), otherwise 400.

400 for an edit that changes nothing, a crop outside the image, a bad angle, a trim on an image (or a turn on a video), a post in the recycle bin or a type that can't be edited; 413 when the result is over the upload limit; 409 `duplicate` when the result is a file that already exists.

### Search syntax (`q`)

Terms are separated by spaces, with at most 40 terms.

| Term | Meaning |
|---|---|
| `blue_sky` | must have the tag, or a tag under it in the tree (`footwear` finds `boots`) |
| `-cloud` | must not have the tag, nor any tag under it |
| `{girl blonde_hair boots}` / `-{boy hat}` | one person in the picture has every tag (or a tag under it in the tree); `girl`, `boy` or `other` inside the braces also sets their kind, and `{boy}` alone matches any boy. With a minus, no one does. A tag that doesn't exist makes `{…}` match nothing, and `-{…}` exclude nothing. |
| `artist:name` | the tag `name`; the prefix (`artist:`, `character:`/`char:`, `copyright:`/`copy:`/`series:`, `meta:`, `general:`/`gen:`) is dropped |
| `rating:g` / `-rating:e` | rating is (not) general, sensitive, questionable or explicit (`g s q e`); repeat to allow several, e.g. `rating:g rating:s` |
| `type:image` / `type:animation` / `type:video` | media kind |
| `date:2024`, `date:2024-03`, `date:2024-03-15` | the post's date is in that period. The date is `file_date` (from the file), or the upload date when there is none. Dates are compared in UTC. |
| `date:2023..2024`, `date:2023-06..`, `date:..2024` | from the start of the first period to the end of the last; either end can be left open |
| `date:>2023-06`, `date:>=2023-06`, `date:<2024`, `date:<=2024` | after or before a period, without or with it |
| `date:>30d`, `date:<7d`, `date:7d` | more than 30 days old; less than 7 days old (a bare age means the same). Units are `h`, `d`, `w`, `mo` and `y`, up to 1000 years. |
| `width:1920`, `height:>1080`, `width:1000..2000` | size in pixels: a value, a comparison (`>`, `>=`, `<`, `<=`) or a range |
| `mpixels:>8`, `mpixels:2..4`, `mpixels:8` | width × height in megapixels, in the same forms. A bare value means about that, to the digits given: `8` is 7.5–8.5 and `8.3` is 8.25–8.35. |
| `ratio:16:9`, `ratio:1.78`, `ratio:>1` | width over height, in the same forms. A bare value has a margin of 0.01, so `ratio:1` is square. |
| `color:red`, `color:2060e0`, `color:#2060e0`, `color:26e` | the picture has that colour (`colour:` works too). The names are `red`, `orange`, `yellow`, `green`, `teal`, `blue`, `purple`, `pink`, `brown`, `black`, `gray` (or `grey`) and `white`. The search widens until it finds a post that fits the rest of the query, up to a limit, so it finds the nearest colours stored and can still come back empty. |
| `-date:…`, `-width:…`, `-color:…` and so on | excludes. A post of unknown size stays in. A bad value is a 400, not a tag search. |
| `order:newest` (default), `order:oldest`, `order:score`, `order:size`, `order:pixels`, `order:longside`, `order:tags`, `order:updated`, `order:random` | sort (`sort:` works too). Add `_asc` or `_desc` to set the direction, e.g. `order:size_asc` for the smallest files first. `pixels` (or `mpixels`) is width × height; `longside` is the longer of the two; posts of unknown size sort last in both. `tags` is the tag count; `updated` is the last edit. |
| `status:deleted` | browse the recycle bin |
| `exact:true` | take tags as typed, without correcting unknown ones. Any other value is an ordinary tag name. |
| `text:hello` / `text:"no parking"` / `-text:sale` | the text OCR read in the image has (or doesn't have) all these words, in any case. Quotes keep several words in one term; several `text:` terms all apply. Matches whole words (`simple` full-text search), at most 200 characters per term. |

Tags are normalised: trimmed, lower-cased, spaces and underscores folded to one `_`, and at most 128 characters.

A tag that is neither a tag nor an alias is corrected to the closest tag on at least one post, in included, excluded and `{...}` terms alike. It may be off by one edit up to 5 characters, two up to 11 and three beyond that, where a swap of neighbouring letters is one edit. Names under 3 characters are left alone, and digits must match exactly, so `4k` never becomes `8k`. At most 8 unknown names per query are looked up. A search and the viewer's neighbours and Explore all correct alike.

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
| GET | `/tag-corrections?q=a+b` | | `TagCorrection[]`: "did you mean" for a whitespace-separated list being typed. The same rules as a search, except that tags on no post can be suggested. Names that are known, have a category prefix (`artist:x`) or aren't valid are skipped. |
| GET | `/tags/{name}` | | `TagDetail {tag, aliases, implies, implied_by, ancestors, children}`. `ancestors` is the parent chain, top first; `children` the tags directly under it. An alias resolves to its tag. 404 if neither exists. |
| PATCH | `/tags/{name}` | `TagUpdate {category?, parent?}` | `Tag`. Categories: `general`, `artist`, `character`, `copyright`, `meta`. `parent` names the tag to put it under (created if needed); `""` moves it to the top. Missing fields stay as they are, and a missing `{name}` is created. 409 `parent_cycle` if the parent is already under it; 400 for itself. Any signed-in user. |
| POST | `/tags/{name}/aliases` | `{alias}` | `TagDetail`. Admin only. If a tag called `alias` exists, it is merged in: its posts, aliases, implications and child tags move to `{name}`, which also takes its parent if it has none, and it is deleted. 400 if `alias` is the tag itself. |
| DELETE | `/tags/{name}/aliases/{alias}` | | `TagDetail`. Admin only. |
| POST | `/tags/{name}/implications` | `{implies}` | `TagDetail`. Admin only. Missing tags are created. Posts that already have `{name}` get the implied tags (transitively). 409 `implication_cycle` if `implies` already implies `{name}`; 400 if they are the same tag. |
| DELETE | `/tags/{name}/implications/{implied}` | | `TagDetail`. Admin only. Posts keep the tags they already got. |

Tags form a tree through their parents. Searching a tag finds posts with any tag below it, and the tops of the tree are the sections the viewer groups general tags under. At first start the server seeds a starter tree of Danbooru-style tags (sections `people`, `hair`, `eyes`, `skin`, `body`, `expression`, `pose`, `clothing`, `setting`, `style`, with groups such as `clothing › footwear › boots`); tags that already have a parent keep it. When tagging, a category prefix such as `artist:name` tags `name` and sets its category.

Tags are resolved the same way everywhere: an alias in a post's tags or in a search means its tag, and implied tags are added whenever tags are set on a post. Implied tags are stored on the post, so they show up in its tag list and in searches. Path segments are percent-encoded.

## Explore

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/explore?days=&q=` | | `Explore {days, trending, tags, on_this_day, random, corrected?}`. `days` is the window, 1 to 365 (default 7). `trending`: up to 24 posts with the most net upvotes cast or changed in the window (only those above zero). `tags`: up to 20 `TagTrend {tag, recent}`, the tags most used on posts added in the window, without the tags `q` requires. `on_this_day`: up to 12 posts added on today's month and day (UTC) in earlier years, newest first. `random`: 12 random posts. `q` uses the search syntax and narrows every section; its `order:` and `status:` are ignored, and `corrected` is as in `/posts`. 400 for a bad query. |

## Other boorus

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/remote/search` | `RemoteSearch {api, base, tags?, page?, login?, api_key?}` | `RemotePage {posts, more}` |

The server searches another booru for you, so the app needs no cross-site access and the same address rules as `POST /posts/import` apply (public addresses only unless the server allows private ones; off when URL import is off).

- `api` is `danbooru`, `gelbooru` (Gelbooru 0.2 and forks such as Safebooru and Rule34's `api.rule34.xxx`), `moebooru` (yande.re, Konachan), `e621` (e621, e926) or `blombooru` (Danbooru's API, but its `s` rating means safe).
- `base` is the site's http(s) address without query or credentials; a trailing slash is dropped. `tags` is in the site's own syntax. `page` starts at 1 and goes up to 1000. Each page asks for 40 posts.
- `login` and `api_key` are the account on that site, sent only to it, in the query as it names them (`user_id` for Gelbooru, `password_hash` for Moebooru). They are used only when both are given, and are never stored or logged by the server. Each field is at most 512 bytes.
- A `RemotePost` is `{id, page_url, preview_url, file_url, tags, rating, width, height}`. `tags` holds only tags valid here (the rest are dropped); `rating` is mapped to StarBooru's scale. Posts without a file the account may see are left out. `more` is true when the site sent a full page.
- Import results with `POST /imports` (in the background) or `POST /posts/import` (one, waiting for it), `url` = `file_url` and `source` = `page_url`.

Errors:
- 400 `bad_request`: a bad request, an address the server won't reach, URL import turned off, a site that wants an account (401/403 from it), or a search it refused (400/422, e.g. Danbooru's two-tag limit without an account).
- 429 `rate_limited` with `Retry-After: 5` when the site is busy (429/503).
- 502 `fetch_failed` when the site errors, times out, or answers with something that isn't that API's search.

## Background imports

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/imports` | `ImportBatch {items: [ImportRequest]}`, 1 to 500 items | `ImportJob[]`, all `queued` |
| GET | `/imports` | | `ImportJob[]`: the caller's imports still waiting or running, and those finished in the last 24 hours, newest first |
| DELETE | `/imports/{id}` | | 204; cancels a waiting import or hides a finished one. 409 `running` while it downloads; 404 if it isn't yours. |
| DELETE | `/imports` | | 204; hides the caller's finished imports |

The server downloads each address on its own, so an import goes on when every app is closed and survives a restart (an import a restart interrupts starts over). Each item follows the rules of `POST /posts/import`; the whole batch is refused (400) when one address is not http(s), has credentials, is on a private network (unless the server allows it), has a bad tag, or URL import is off. 409 `too_many_imports` past 2000 waiting imports per account. Two imports run at once, server-wide, oldest first. Finished imports are deleted after 7 days.

`ImportJob` is `{id, url, source, tags, rating, state, post_id, error, created_at, finished_at}`. `state` is `queued`, `running`, `done` (`post_id` is the new post), `duplicate` (the file was already here; `error` says which post) or `failed` (`error` says why, e.g. the site answered 404, the file is over the upload limit, the storage limit, a disabled account). File uploads (`POST /posts`) are sent by the app, so they stop when it closes.

## Library imports

Moves a whole library from another self-hosted booru. The server walks that site by itself, one import at a time and oldest first, so an import goes on with every app closed and picks up where it was after a restart.

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/library-imports/check` | `LibrarySite` | `LibraryCheck {total?, rules}`. Reads the first page, once. `total` is missing when the site doesn't say. `rules` says whether aliases and implications would come across. |
| POST | `/library-imports` | `LibrarySite` | 201, `LibraryImport`, `queued`. 403 past 5 imports waiting or running per account. |
| GET | `/library-imports` | | `LibraryImport[]`: the caller's latest 50, newest first |
| POST | `/library-imports/{id}/stop` | | `LibraryImport`. What came across stays. |
| POST | `/library-imports/{id}/retry` | `LibraryRetry {login?, secret?}` | `LibraryImport`, `queued`. Walks the site again, trying the failed posts and finding new ones, and skips what already came across. The key has to be given again. 409 `running` while it's going. |
| DELETE | `/library-imports/{id}` | | 204; forgets the import, its posts stay. 409 `running` while it's going. |
| GET | `/library-imports/{id}/failures` | | `LibraryFailure[] {remote_id, error}`, at most 500 |

- `LibrarySite` is `{source, base, login?, secret?, unrated}`. `source` is `szurubooru`, `shimmie2`, `danbooru`, `stash`, `blombooru`, `monbooru` or `lanbooru`. `base` is the site's http(s) address. `unrated` is the rating for posts that come without one.
- `login` and `secret` are sent only to that site and never returned or logged. They stay in the database only while the import is waiting or running, so a restart can go on, and are erased when it ends.
- `LibraryImport` is `{id, source, base, state, phase, total?, imported, merged, failed, rules, note?, error?, created_at, finished_at?}`. `state` is `queued`, `running`, `done`, `stopped` or `failed`. `phase` is `rules`, `posts` or `groups`. `merged` counts files that were already here, whose tags and likes are added to the post. `note` says what was left out and `error` why it stopped; the server writes both in English.
- An administrator's import can reach the server's own network. Anyone else's follows the rules of `POST /posts/import`. Only an administrator's import brings aliases and implications, as they apply to everyone. With URL import off, checking, starting or retrying one answers 400.

Errors: 400 `bad_request` for a bad address or key, or an address the server won't reach; 502 `fetch_failed` when the site can't be reached, refuses the account or doesn't answer like that system.

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
| GET | `/shared/{token}` | | **No auth.** `SharedView {title, server, expires_at, posts}`, where `posts` are `SharedPost {kind, mime, width, height, duration_ms, file_url, thumb_url, sample_url}` in the link's order. Posts moved to the recycle bin since are left out. 404 for a malformed, unknown, expired or removed link, or one whose owner is disabled. Answers with `Cache-Control: no-store` and `Referrer-Policy: no-referrer`. |

The page to give out is `/s/{token}` on the server, which the web app serves (`starbooru_core::share::share_path`).

## Media

```text
GET /media/{original|thumb|sample}/{sha256}.{ext}?exp=<unix>&sig=<hex>
```

Media URLs come from the API, so never build one by hand. Invalid or expired signatures get a 403. Range requests are supported. Responses are cacheable (`private, max-age=21600, immutable`).
