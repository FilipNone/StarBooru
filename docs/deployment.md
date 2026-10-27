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
6. **Backups.** In *Volume Backups*, back up both named volumes:
   - `pgdata` (the database: tags, scores, users);
   - `media` (the files).

   You need both to restore. For a consistent database copy you can also run `pg_dump` from the `db` container on a schedule.

Desktop and Android users sign in with the same URL (`https://your-domain`).

### Upload size

`STARBOORU_MAX_UPLOAD_MB` (default 512) is the server's limit. Dokploy's Traefik doesn't cap request bodies by default. If you put another proxy (nginx, Cloudflare) in front, raise its limit too. Cloudflare's free plan caps request bodies at 100 MB.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | required | PostgreSQL URL (the compose file builds it from `POSTGRES_PASSWORD`) |
| `STARBOORU_SECRET` | required | At least 32 characters; signs media URLs. Changing it invalidates existing media links, which clients refresh on reload. |
| `STARBOORU_NAME` | `StarBooru` | Instance name shown at sign-in |
| `STARBOORU_ALLOW_REGISTRATION` | `false` | Allow sign-ups after the first (admin) account |
| `STARBOORU_MAX_UPLOAD_MB` | `512` | Largest upload, 1 to 16384 |
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
