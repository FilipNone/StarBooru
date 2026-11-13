# StarBooru server image: the API, media storage and the web app in one container.
# Works with BuildKit and the legacy builder (no cache mounts).
#
# AI_GPU picks what local AI can use besides the processor:
#   none    processor only, x86-64 and ARM64 (the default)
#   vulkan  AMD, Intel and NVIDIA cards through their Vulkan drivers (x86-64)
#   nvidia  NVIDIA cards through CUDA, faster and about 3 GB larger (x86-64)

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

# The version the app and the server show: a release's tag, or Cargo's when empty.
ARG STARBOORU_VERSION=
RUN dx bundle --web --release -p starbooru-app --out-dir /out/web
RUN cargo build --release --locked -p starbooru-server -p starbooru-ai-worker \
    && cp target/release/starbooru-server /out/starbooru-server \
    && mkdir -p /out/ai \
    && cp target/release/starbooru-ai-worker /out/ai/starbooru-ai-worker
# A .br and a .gz of each text file, which the server sends as they are.
RUN /out/starbooru-server precompress /out/web/public

# ONNX Runtime, which runs the models, pinned by version and hash.
ARG AI_GPU=none
ARG ORT_VERSION=1.31.1
ARG ORT_SHA256_X86_64=c849e36a1ef3e22d4ff217b9d1ce4a3cf5b9467b975610913ae959ad5b16bdc9
ARG ORT_SHA256_AARCH64=5317505e2da99be65f8e2775ba3c2afd6d2961fd78aba1bd8f56159f48820c78
ARG ORT_SHA256_CUDA=b2146862e691f7e338f5aff9c7a9a5567c2ca57cf01442a1adb3a13bf62d9095
RUN set -eux; \
    case "$AI_GPU:$(uname -m)" in \
      none:x86_64|vulkan:x86_64) pkg="onnxruntime-linux-x64-$ORT_VERSION"; sha="$ORT_SHA256_X86_64" ;; \
      none:aarch64) pkg="onnxruntime-linux-aarch64-$ORT_VERSION"; sha="$ORT_SHA256_AARCH64" ;; \
      nvidia:x86_64) pkg="onnxruntime-linux-x64-gpu_cuda13-$ORT_VERSION"; sha="$ORT_SHA256_CUDA" ;; \
      *) echo "AI_GPU=$AI_GPU isn't available on $(uname -m)" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -A starbooru-build -o /tmp/ort.tgz \
      "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/${pkg}.tgz"; \
    echo "$sha  /tmp/ort.tgz" | sha256sum -c -; \
    mkdir /tmp/ort; \
    tar -xzf /tmp/ort.tgz -C /tmp/ort --strip-components=1; \
    cp "/tmp/ort/lib/libonnxruntime.so.$ORT_VERSION" /out/ai/libonnxruntime.so; \
    if [ "$AI_GPU" = nvidia ]; then \
      cp /tmp/ort/lib/libonnxruntime_providers_cuda.so /tmp/ort/lib/libonnxruntime_providers_shared.so /out/ai/; \
    fi; \
    rm -rf /tmp/ort /tmp/ort.tgz

# The worker for graphics cards, tried before the processor's.
RUN set -eux; \
    case "$AI_GPU" in \
      vulkan) \
        cargo build --release --locked -p starbooru-ai-worker --no-default-features --features webgpu; \
        cp target/release/starbooru-ai-worker /out/ai/starbooru-ai-worker-gpu; \
        cp -L target/release/libwebgpu_dawn.so /out/ai/ ;; \
      nvidia) \
        cargo build --release --locked -p starbooru-ai-worker --features cuda; \
        cp target/release/starbooru-ai-worker /out/ai/starbooru-ai-worker-gpu ;; \
    esac

# The CUDA libraries ONNX Runtime needs, from NVIDIA's wheels on PyPI. The
# container gets the driver itself from the NVIDIA Container Toolkit.
RUN set -eux; \
    [ "$AI_GPU" = nvidia ] || exit 0; \
    cd /tmp; \
    for wheel in \
      "21721774d8e7044debec3034ecc1a8cd22e173eb6dae5d7f5f8a014852382894 https://files.pythonhosted.org/packages/04/36/c6f716dbd080fa030e6056261f238fb82c2b78d94e5074460740f61384b3/nvidia_cublas-13.8.2.12-py3-none-manylinux_2_27_x86_64.whl" \
      "9641f797da20ce1dd8e779b6e96d08cf9ba564cec8e8225458811ee26423f3a5 https://files.pythonhosted.org/packages/98/8a/3431271f6344874b8f1ac03f16b3d679c91493f8da63f716160403e6d0a0/nvidia_cuda_runtime-13.4.92-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl" \
      "25c3457ae7a224fdd484dab90b0fc5dc0e842fab5db3012afa4a5bd2af4eb7e5 https://files.pythonhosted.org/packages/07/73/3ee8e5b4cb891401e603ffd3a59b35c6afe785fd2de123afe7c7029603dc/nvidia_curand-10.4.4.72-py3-none-manylinux_2_27_x86_64.whl" \
      "9677e76f21862eb5da7ee5ed69d544738b2d8b5c3ce7e5ec125c5592e6cdbdc8 https://files.pythonhosted.org/packages/af/75/96ea5c5368eb595c39d629cde08a66227a864e71ccb0e593add2612bb952/nvidia_cudnn_cu13-9.27.0.42-py3-none-manylinux_2_27_x86_64.whl"; \
    do \
      set -- $wheel; \
      curl -fsSL -A starbooru-build -o wheel.zip "$2"; \
      echo "$1  wheel.zip" | sha256sum -c -; \
      python3 -c 'import os,zipfile; z=zipfile.ZipFile("wheel.zip"); [open("/out/ai/"+os.path.basename(n),"wb").write(z.read(n)) for n in z.namelist() if ".so" in os.path.basename(n)]'; \
      rm wheel.zip; \
    done


FROM debian:trixie-slim

ARG AI_GPU=none

# ffmpeg makes thumbnails for video, AVIF and JPEG XL; pg_dump makes database backups;
# tesseract reads text in images (OCR): English, and Japanese across and down for manga.
# For more OCR languages add their packages here, e.g. tesseract-ocr-ita or
# tesseract-ocr-deu, and list them in STARBOORU_OCR_LANGS (eng+ita).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg postgresql-client \
       tesseract-ocr tesseract-ocr-eng tesseract-ocr-jpn tesseract-ocr-jpn-vert \
    && rm -rf /var/lib/apt/lists/*
# Vulkan drivers for the graphics card worker. Without the software renderer,
# which is slower than the processor worker, a server with no card finds none.
RUN set -eux; \
    [ "$AI_GPU" = vulkan ] || exit 0; \
    apt-get update; \
    apt-get install -y --no-install-recommends libvulkan1 mesa-vulkan-drivers; \
    rm /usr/share/vulkan/icd.d/lvp_icd.json; \
    rm -rf /var/lib/apt/lists/*
# A fresh named volume copies this directory's ownership, so the server can write to it.
RUN useradd --system --uid 10001 --user-group --no-create-home --home-dir /data starbooru \
    && mkdir -p /data \
    && chown starbooru:starbooru /data

COPY --from=build /out/starbooru-server /app/starbooru-server
COPY --from=build /out/web/public /app/web
COPY --from=build /out/ai/ /app/

ENV STARBOORU_BIND=0.0.0.0:8080 \
    LD_LIBRARY_PATH=/app \
    STARBOORU_DATA_DIR=/data \
    STARBOORU_WEB_DIR=/app/web

USER starbooru
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=90s --retries=3 \
    CMD ["/app/starbooru-server", "healthcheck"]
ENTRYPOINT ["/app/starbooru-server"]
