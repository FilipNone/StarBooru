# StarBooru

A self-hosted personal booru for organising and tagging private image and video collections. It is written in Rust.

- **Server:** one Docker image with the API, the media store and the web interface, backed by PostgreSQL. It's ready for [Dokploy](https://dokploy.com).
- **Apps:** Windows, Linux, macOS and Android apps, built from the same Dioxus code as the web interface. They connect to your server.
- **Features:**
  - tag search with exclusions, ratings and sorting;
  - autocomplete;
  - votes;
  - drag-and-drop upload with duplicate detection;
  - a recycle bin;
  - light and dark themes with a custom accent colour.

## Deploy

See [docs/deployment.md](docs/deployment.md). In short, on Dokploy:

1. Create a Compose service from this repository.
2. Set `POSTGRES_PASSWORD` and `STARBOORU_SECRET` (see `.env.example`).
3. Add a domain for service `starbooru`, port `8080`.
4. Deploy, open the site and create the first (admin) account.

Locally:

```sh
cp .env.example .env   # fill in the secrets
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d --build
```

## Develop

See [docs/development.md](docs/development.md). Contributors and AI agents start with [AGENTS.md](AGENTS.md).

| Doc | |
|---|---|
| [architecture.md](docs/architecture.md) | How it fits together |
| [api.md](docs/api.md) | HTTP API and search syntax |
| [roadmap.md](docs/roadmap.md) | What's done and what's next |
