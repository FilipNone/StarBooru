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
| `DATABASE_URL` | required | PostgreSQL URL (the compose file builds it from `POSTGRES_PASSWORD`) |
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
| `STARBOORU_URL_IMPORT` | `on` | Import from URL: `on` (public addresses only), `private` (also private-network addresses, e.g. a NAS; anyone with an account can then make the server fetch from your LAN), `off` |
| `STARBOORU_AI_URL` | unset | Base URL of an OpenAI-compatible API for tag suggestions, such as `https://api.openai.com/v1`, `https://openrouter.ai/api/v1`, `https://generativelanguage.googleapis.com/v1beta/openai` or a local `http://ollama:11434/v1`. Unset turns AI off. Thumbnails of posts are sent to it; a local server keeps them on your network. Admins can change it in Administration › Server. |
| `STARBOORU_AI_MODEL` | unset | A vision model, such as `gpt-4o-mini` or `llava` |
| `STARBOORU_AI_KEY` | unset | API key, sent as a bearer token. Local servers usually need none. Never shown to clients or logged. |
| `STARBOORU_AI_AUTOTAG` | `false` | Add suggested tags to every new upload in the background (two at a time) |
| `STARBOORU_TRUST_PROXY` | `false` (`true` in `docker-compose.yml`) | Take the client address from the last `X-Forwarded-For` entry, for the sign-in limits. Turn it on only behind a reverse proxy that sets the header (Traefik does); otherwise clients could pick their own address. |
| `STARBOORU_BIND` | `0.0.0.0:8080` | Listen address |
| `STARBOORU_DATA_DIR` | `/data` in the image | Media storage |
| `STARBOORU_WEB_DIR` | `/app/web` in the image | Compiled web app; unset means the server runs API-only |
| `RUST_LOG` | `info,tower_http=info,sqlx=warn` | Log filter |

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
