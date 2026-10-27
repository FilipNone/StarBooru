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

## Configuration

Admins can change the name, open registration, the recycle-bin days and the backup schedule in the app (Settings › Administration). Those changes apply at once and override the variables below.

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | required | PostgreSQL URL (the compose file builds it from `POSTGRES_PASSWORD`) |
| `STARBOORU_SECRET` | required | At least 32 characters; signs media URLs. Changing it invalidates existing media links, which clients refresh on reload. |
| `STARBOORU_NAME` | `StarBooru` | Instance name shown at sign-in |
| `STARBOORU_ALLOW_REGISTRATION` | `false` | Allow sign-ups after the first (admin) account |
| `STARBOORU_MAX_UPLOAD_MB` | `512` | Largest upload, 1 to 16384 |
| `STARBOORU_TRASH_DAYS` | `0` | Purge recycle-bin posts (and their files) after this many days; `0` keeps them until emptied by hand |
| `STARBOORU_BACKUP_DAYS` | `1` | Days between automatic database backups, 0 to 365; `0` turns them off |
| `STARBOORU_BACKUP_KEEP` | `7` | Automatic backups kept, 1 to 100 |
| `STARBOORU_BACKUP_DIR` | `STARBOORU_DATA_DIR/backups` | Where backups are written |
| `STARBOORU_PG_DUMP` | `pg_dump` | pg_dump (installed in the image); `off` turns backups off |
| `STARBOORU_FFMPEG` | `ffmpeg` | ffmpeg for video, AVIF and JPEG XL thumbnails (installed in the image); `off` disables it |
| `STARBOORU_FFPROBE` | `ffprobe` | ffprobe, for video size and length |
| `STARBOORU_URL_IMPORT` | `on` | Import from URL: `on` (public addresses only), `private` (also private-network addresses, e.g. a NAS; anyone with an account can then make the server fetch from your LAN), `off` |
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
