# HTTP API (v1)

All endpoints are under `/api/v1`. Bodies are JSON unless noted. The Rust types are in `crates/core/src/model.rs`, and `starbooru-client` wraps every call.

**Auth.** Send `Authorization: Bearer <token>`. Every endpoint except `info`, `auth/register` and `auth/login` requires it.

**Errors.** Errors are returned as `{"code": "...", "message": "..."}` with a matching status:

| Status | Meaning |
|---|---|
| 400 | bad input |
| 401 | missing or expired token |
| 403 | not allowed |
| 404 | not found |
| 409 | conflict (`duplicate`, `username_taken`, `deleted`, `not_deleted`) |
| 413 | upload too large |
| 415 | unsupported file type |
| 429 | rate limited; the client honours `Retry-After` |
| 500 | internal error (details only in the server log) |

## Server

| Method | Path | |
|---|---|---|
| GET | `/info` | `ServerInfo`: `name`, `version`, `has_users`, `registration_open`, `max_upload_bytes` |
| GET | `/healthz` (not under `/api/v1`) | `ok` when the database answers |

## Accounts

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/auth/register` | `{username, password}` | `AuthResponse {token, user}`. The first account becomes admin. Later accounts need `STARBOORU_ALLOW_REGISTRATION`. |
| POST | `/auth/login` | `{username, password}` | `AuthResponse` |
| POST | `/auth/logout` | | 204; revokes this token |
| GET | `/auth/me` | | `User` |

Account rules:
- Usernames are 2 to 32 characters of `A–Z a–z 0–9 _ . -` and are unique regardless of case.
- Passwords are 8 characters or more.
- Tokens last 90 days.

## Posts

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/posts?q=&page=&limit=` | | `PostPage {posts, page, limit, has_more}`. `page` starts at 1 (at most 10000); `limit` is at most 200, default 60. |
| POST | `/posts` | multipart: `file` (required), `tags` (space-separated), `rating` (`g`/`s`/`q`/`e`), `source` | `Post`, or 409 `duplicate` if the same file exists |
| GET | `/posts/{id}` | | `Post` |
| GET | `/posts/{id}/neighbors?q=` | | `Neighbors {prev, next}`: the ids just before and after the post in the results of search `q` (same filters and order), or null at either end. Both are null for `order:random` or when `q` matches nothing. |
| PATCH | `/posts/{id}` | `PostUpdate {tags?, rating?, source?}` | `Post`. `tags` replaces the whole list. |
| DELETE | `/posts/{id}` | | 204; moves the post to the recycle bin (admin or uploader) |
| POST | `/posts/{id}/restore` | | `Post` |
| POST | `/posts/{id}/purge` | | 204; permanently deletes a post that is in the recycle bin, with its files (admin or uploader). 409 `not_deleted` if the post is live. |
| PUT | `/posts/{id}/vote` | `{value: -1 \| 0 \| 1}` | `Post` with the new `score` and `my_vote` |

A `Post` carries `file_url` and `thumb_url`. These are signed paths relative to the server, valid for at least 6 hours. Fetch them without the bearer token. `thumb_url` is null when no thumbnail exists.

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

## Media

```text
GET /media/{original|thumb}/{sha256}.{ext}?exp=<unix>&sig=<hex>
```

Media URLs come from the API, so never build one by hand. Invalid or expired signatures get a 403. Range requests are supported. Responses are cacheable (`private, max-age=21600, immutable`).
