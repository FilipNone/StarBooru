# Deployment

StarBooru ships as one image that contains the server and the web app, plus a PostgreSQL container. The repository's `docker-compose.yml` is ready for Dokploy.

## Dokploy

1. **Create a Compose service.** In your project, choose *Create Service › Compose*. Set *Provider* to this Git repository, the branch, and *Compose Path* `./docker-compose.yml`.
2. **Environment.** In the *Environment* tab, paste the variables from `.env.example` and fill them in:
   ```sh
   openssl rand -hex 24   # POSTGRES_PASSWORD (letters and digits only; it goes into a URL)
   openssl rand -hex 32   # STARBOORU_SECRET
   ```
3. **Domain.** In the *Domains* tab, add your domain for service **`starbooru`**, port **`8080`**, with HTTPS (Let's Encrypt). Dokploy adds the Traefik labels and network. Don't publish host ports.
4. **Deploy.** The first build compiles everything and takes several minutes. Later deploys reuse Docker's layer cache where they can.
5. **Create the admin account.** Open `https://your-domain` and choose *Create account*. The first account becomes admin. Registration then closes unless `STARBOORU_ALLOW_REGISTRATION=true`.
6. **Backups.** StarBooru backs the database up itself (see [Backups](#backups)) into the `media` volume. In *Volume Backups*, back up the `media` volume off the server: it then holds the files and the database backups. Backing up `pgdata` as well does no harm, but a copy of a running database's files may not restore cleanly; the `pg_dump` archives always do.

Desktop and Android users sign in with the same URL (`https://your-domain`).

### Upload size

`STARBOORU_MAX_UPLOAD_MB` (default 512) is the server's limit. Dokploy's Traefik doesn't cap request bodies by default. If you put another proxy (nginx, Cloudflare) in front, raise its limit too. Cloudflare's free plan caps request bodies at 100 MB.

### Video thumbnails

The image ships ffmpeg (most of its roughly 750 MB). The server uses it for thumbnails, size and length of videos, AVIF and JPEG XL. Without it, those posts show a placeholder. Each start retries posts that still lack a thumbnail, so installing ffmpeg later fills them in.

## Backups

The server writes `pg_dump` archives (custom format) of the database to `STARBOORU_DATA_DIR/backups`:

- **Automatically,** every `STARBOORU_BACKUP_DAYS` days (default 1), keeping the newest `STARBOORU_BACKUP_KEEP` (default 7). The first check runs five minutes after start-up, then hourly.
- **By hand,** from Settings › Administration › Backups. Backups made by hand are kept until an admin deletes them.

Admins download and delete backups on the same page and change the schedule there. Archives hold every account's password hash, so only admins signed in to the app can reach them (API keys can't).

A backup holds the database only. The media files are immutable and named by their hash, so copying the data folder (or the `media` volume) is all they need.

The image ships the PostgreSQL 17 client. `pg_dump` must be at least as new as the database server, so update the image too if you move the `db` service to a newer PostgreSQL. Elsewhere, install the client or point `STARBOORU_PG_DUMP` at it.

### Restoring

1. Stop the `starbooru` service. Leave `db` running.
2. Copy the archive into the `db` container and load it over the current database:
   ```sh
   docker cp starbooru-20261027-031500.dump <db-container>:/tmp/restore.dump
   docker exec <db-container> pg_restore --clean --if-exists --no-owner \
       -U starbooru -d starbooru /tmp/restore.dump
   ```
3. Put the media files back if they were lost, into the `media` volume, as they were.
4. Start `starbooru` again. Migrations newer than the backup run at start-up.

## Sign-in with an identity provider

StarBooru can also sign people in with an OpenID Connect provider such as Authentik, Keycloak, Authelia or Google. Passwords keep working next to it.

1. At the provider, create an OAuth2/OpenID application: confidential client, authorization code flow, scopes `openid profile email`. Set the redirect URI to `https://your-domain/api/v1/auth/oidc/callback`.
2. Set `STARBOORU_PUBLIC_URL=https://your-domain`, `STARBOORU_OIDC_ISSUER` (the provider's issuer URL, the one that serves `/.well-known/openid-configuration`), `STARBOORU_OIDC_CLIENT_ID`, `STARBOORU_OIDC_CLIENT_SECRET` and, for the button, `STARBOORU_OIDC_NAME`.
3. Redeploy. The sign-in screen shows *Sign in with …*.

Someone who signs in this way for the first time gets a new account when registration is open (or when the server has no accounts yet, as admin). With registration closed, an admin keeps control: people sign in once with a password, then link the provider account in Settings › Ways to sign in. Accounts made by the provider have no password until they set one in Settings.

The desktop and Android apps open the system browser and receive the answer on a loopback address, so nothing else needs registering at the provider.

## Configuration

Admins can change the name, open registration, the recycle-bin days and the backup schedule in the app (Settings › Administration). Those changes apply at once and override the variables below.

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | required | PostgreSQL URL (the compose file builds it from `POSTGRES_PASSWORD`). The migrations add the `pg_trgm` extension. That needs PostgreSQL 13 or newer, where the database's owner can add it, or a superuser on anything older. The bundled container is fine. |
| `STARBOORU_SECRET` | required | At least 32 characters; signs media URLs. Changing it invalidates existing media links, which clients refresh on reload. |
| `STARBOORU_NAME` | `StarBooru` | Instance name shown at sign-in |
| `STARBOORU_ALLOW_REGISTRATION` | `false` | Allow sign-ups after the first (admin) account |
| `STARBOORU_MAX_UPLOAD_MB` | `512` | Largest upload, 1 to 16384 |
| `STARBOORU_SAMPLES` | `true` | Make smaller JPEG copies of big images for viewing (see `architecture.md` › *Media storage*). Turned on later, copies are made for older posts at the next start. |
| `STARBOORU_AUTO_TAGS` | `true` | Tag posts with what the file shows, in the `meta` category: `lowres`, `highres`, `absurdres`, `wide_image`, `tall_image`, `animated`, `video`, `sound`, `long_video` (see `architecture.md` › *Automatic meta tags*). Each post is tagged once; turned on later, older posts are tagged at the next start. Admins can override it in Administration › Server. |
| `STARBOORU_TRASH_DAYS` | `0` | Purge recycle-bin posts (and their files) after this many days; `0` keeps them until emptied by hand |
| `STARBOORU_PUBLIC_URL` | unset | The address people open StarBooru at, e.g. `https://booru.example.com`. Needed for provider sign-in. |
| `STARBOORU_OIDC_ISSUER` | unset | OpenID Connect issuer URL (https). With the client id and secret, turns provider sign-in on. |
| `STARBOORU_OIDC_CLIENT_ID` | unset | Client id from the provider |
| `STARBOORU_OIDC_CLIENT_SECRET` | unset | Client secret from the provider |
| `STARBOORU_OIDC_NAME` | `SSO` | Provider name on the *Sign in with …* button |
| `STARBOORU_BACKUP_DAYS` | `1` | Days between automatic database backups, 0 to 365; `0` turns them off |
| `STARBOORU_BACKUP_KEEP` | `7` | Automatic backups kept, 1 to 100 |
| `STARBOORU_BACKUP_DIR` | `STARBOORU_DATA_DIR/backups` | Where backups are written |
| `STARBOORU_PG_DUMP` | `pg_dump` | pg_dump (installed in the image); `off` turns backups off |
| `STARBOORU_FFMPEG` | `ffmpeg` | ffmpeg for video, AVIF and JPEG XL thumbnails (installed in the image); `off` disables it |
| `STARBOORU_FFPROBE` | `ffprobe` | ffprobe, for video size and length |
| `STARBOORU_OCR` | `true` | Read text in still images with tesseract (installed in the image), for `text:` search and the viewer's *Text in image*. Runs on the server only; no image leaves it. `off` turns it off; a path picks another tesseract. Without tesseract it is quietly off (logged once at start). Older posts are read in the background at each start. |
| `STARBOORU_OCR_LANGS` | `eng` (`eng+jpn+jpn_vert` in `docker-compose.yml`) | tesseract languages, joined with `+`, e.g. `eng+ita`. The image has English and Japanese (`jpn` across, `jpn_vert` down, for manga); fewer languages read faster. For another language add its package (`tesseract-ocr-ita`) in the `Dockerfile` first. |
| `STARBOORU_URL_IMPORT` | `on` | Import from URL: `on` (public addresses only), `private` (also private-network addresses, e.g. a NAS; anyone with an account can then make the server fetch from your LAN), `off` |
| `STARBOORU_AI_URL` | unset | Base URL of a hosted OpenAI-compatible API, such as `https://openrouter.ai/api/v1`, `https://api.openai.com/v1` or `https://generativelanguage.googleapis.com/v1beta/openai`. Unset turns the hosted provider off. Thumbnails of posts, and the text of images being translated, are sent to it. Admins can change every AI setting in Administration › Server. |
| `STARBOORU_AI_MODEL` | unset | A vision model, such as `google/gemini-2.5-flash-lite` or `gpt-4o-mini` |
| `STARBOORU_AI_KEY` | unset | API key, sent as a bearer token. Never shown to clients or logged. |
| `STARBOORU_AI_AUTOTAG` | `false` | Add suggested tags to every new upload in the background (two at a time) |
| `STARBOORU_LOCAL_AI` | `false` | Run AI on this server's hardware through Ollama (see *Local AI* below) |
| `STARBOORU_LOCAL_AI_URL` | `http://127.0.0.1:11434` (`http://ollama:11434` in `docker-compose.yml`) | Ollama's address |
| `STARBOORU_LOCAL_AI_MODEL` | unset | Ollama model; unset picks the preset that suits the hardware |
| `STARBOORU_AI_TAGGING` | see meaning | Where tag suggestions run: `off`, `hosted` or `local`. Unset: hosted when `STARBOORU_AI_URL` is set, else local when it is on, else off. |
| `STARBOORU_AI_TRANSLATION` | see meaning | Where translations run, the same way |
| `STARBOORU_AI_TRANSLATE_TO` | unset | Translate new uploads in which OCR finds text into this language (`en`, `it`, `pt-BR`), drawn over the image (see *Comics and manga* below). Unset: only when someone asks in the viewer. |
| `STARBOORU_TRUST_PROXY` | `false` (`true` in `docker-compose.yml`) | Take the client address from the last `X-Forwarded-For` entry, for the sign-in limits. Turn it on only behind a reverse proxy that sets the header (Traefik does); otherwise clients could pick their own address. |
| `STARBOORU_BIND` | `0.0.0.0:8080` | Listen address |
| `STARBOORU_DATA_DIR` | `/data` in the image | Media storage |
| `STARBOORU_WEB_DIR` | `/app/web` in the image | Compiled web app; unset means the server runs API-only |
| `RUST_LOG` | `info,tower_http=info,sqlx=warn` | Log filter |

## Local AI

Tag suggestions and translation can run on the server's own hardware instead of a hosted provider, through [Ollama](https://ollama.com). Nothing leaves the server and there is no per-image cost. Each tool runs where the admin picks (Administration › Server › AI): off, hosted or local, so tagging can stay local while translation uses a hosted model, for example.

1. Start Ollama next to StarBooru with the `local-ai` profile, and set `STARBOORU_LOCAL_AI=true`:

   ```sh
   docker compose --profile local-ai up -d                                   # processor only
   docker compose -f docker-compose.yml -f docker-compose.gpu.yml --profile local-ai up -d   # with a GPU
   ```

   `docker-compose.gpu.yml` passes `/dev/dri` through, so Ollama uses the GPU through Vulkan: integrated Intel and AMD graphics and AMD Radeon cards. For an NVIDIA card, install the NVIDIA Container Toolkit and give the `ollama` service a `deploy.resources.reservations.devices` entry with `driver: nvidia` and `capabilities: [gpu]` instead. On Dokploy, add the same service to the compose file.

2. In Administration › Server › AI, turn *Use local AI* on. The card shows the server's processor, memory and graphics, and for each preset whether it **runs well**, **runs slowly**, is **too slow** or **won't fit**, and on what (graphics card, integrated graphics or processor). The preset that suits the machine is marked *Recommended*.
3. Download the model from the card, save, and use *Test the speed*: it tags a small generated image and reports the time and how much of the model sits in GPU memory. The first run also loads the model.

| Preset | Model | Download | For |
|---|---|---|---|
| Small | `qwen3-vl:2b-instruct` | 1.9 GB | Raspberry Pi 5 (8 GB) and other small boards; about a minute per image on the processor |
| Medium | `qwen3-vl:4b-instruct` | 3.3 GB | Laptops and mini PCs with integrated graphics and 8 GB or more |
| Large | `qwen3-vl:8b-instruct` | 6.2 GB | Desktops with a graphics card of 8 GB or more |

Any other Ollama model that reads images works too (*Another model*). The check reads the hardware where StarBooru runs; when Ollama runs on another machine, point `STARBOORU_LOCAL_AI_URL` at it and trust the speed test instead. Local answers may take up to five minutes before they time out.

The *Recommended* preset, which is also the one used when no model is picked, counts all of the graphics card's memory, so it doesn't change with what else is running. The fit shown for each preset counts only the memory free right now: on a desktop that also drives a screen, the largest preset may show *slowly, partly on the graphics card* while a game or browser holds video memory.

## Comics and manga

OCR is what finds text in images, for `text:` search and the viewer's *Text in image*. For comic and manga pages, translation goes further: a vision model reads the page and its translation is drawn over each balloon, caption and sound effect.

- In the viewer, the translate button on the image (bottom left) translates the page into the app's language and shows it over the picture; it then toggles the translation on and off. Hovering a balloon shows the original text. Translations are kept per post and language, so each page is translated once.
- To translate new uploads automatically, set *Translate new images automatically into* (Administration › Server › AI) or `STARBOORU_AI_TRANSLATE_TO`. A new image is translated after OCR finds text in it, one at a time, so OCR has to be on. Older posts aren't translated in bulk.
- The image goes to the translation model (fit in 1536 px), unlike the text-only *Translate* under *Text in image*. With local AI it stays on the server; with a hosted provider, the provider sees the page.
- Quality depends on the model. On a generated test page, `google/gemini-2.5-flash` (hosted) and `qwen3-vl:8b-instruct` (local, about 13 s on an 8 GB AMD graphics card) both read every balloon; smaller models miss more text and place boxes less exactly. Very dense pages may need a longer Ollama context (`OLLAMA_CONTEXT_LENGTH=8192` on the `ollama` service), at the cost of more memory.

## Plain Docker Compose

```sh
cp .env.example .env        # fill in the two secrets
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d --build
# → http://localhost:8080
```

## Health

- `GET /healthz` returns `ok` when the database answers.
- The image's `HEALTHCHECK` runs `starbooru-server healthcheck`, so no curl is needed.

The container runs as uid 10001. A fresh named volume gets the right ownership automatically. If you bind-mount a host directory instead, `chown 10001:10001` it.

## Updating

Redeploy. Database migrations run at startup, and the media layout is stable. Back up first when the release notes mention a migration.
