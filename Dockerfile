# StarBooru server image: the API, media storage and the web app in one container.
# Works with BuildKit and the legacy builder (no cache mounts).

ARG RUST_VERSION=1.98

FROM rust:${RUST_VERSION}-trixie AS build

# Prebuilt Dioxus CLI from the upstream release, pinned by version and hash.
ARG DX_VERSION=0.7.10
ARG DX_SHA256_X86_64=4363e4ed2a3f1eb7f4d38d2d59aed59ce43271c44c16b425e92c89a64761fbe7
ARG DX_SHA256_AARCH64=8f1a17d3218700ffbe15e6540d936a178b2556fc801121a31082e3ba4ab9ef55

RUN rustup target add wasm32-unknown-unknown
RUN set -eux; \
    arch="$(uname -m)"; \
    case "$arch" in \
      x86_64) sha="$DX_SHA256_X86_64" ;; \
      aarch64) sha="$DX_SHA256_AARCH64" ;; \
      *) echo "no prebuilt dx for $arch" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -A starbooru-build -o /tmp/dx.tar.gz \
      "https://github.com/DioxusLabs/dioxus/releases/download/v${DX_VERSION}/dx-${arch}-unknown-linux-gnu.tar.gz"; \
    echo "$sha  /tmp/dx.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/dx.tar.gz -C /usr/local/bin dx; \
    rm /tmp/dx.tar.gz; \
    dx --version

WORKDIR /src
COPY . .

RUN dx bundle --web --release -p starbooru-app --out-dir /out/web
RUN cargo build --release --locked -p starbooru-server \
    && cp target/release/starbooru-server /out/starbooru-server


FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# A fresh named volume copies this directory's ownership, so the server can write to it.
RUN useradd --system --uid 10001 --user-group --no-create-home --home-dir /data starbooru \
    && mkdir -p /data \
    && chown starbooru:starbooru /data

COPY --from=build /out/starbooru-server /app/starbooru-server
COPY --from=build /out/web/public /app/web

ENV STARBOORU_BIND=0.0.0.0:8080 \
    STARBOORU_DATA_DIR=/data \
    STARBOORU_WEB_DIR=/app/web

USER starbooru
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=90s --retries=3 \
    CMD ["/app/starbooru-server", "healthcheck"]
ENTRYPOINT ["/app/starbooru-server"]
