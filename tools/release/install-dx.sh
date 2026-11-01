#!/bin/sh
# Installs the prebuilt Dioxus CLI from the upstream release, pinned by version and hash.
#   tools/release/install-dx.sh <platform archive> <sha256>
set -eu

version=0.7.10
archive="dx-$1"
curl -fsSL -A starbooru-build -o "$archive" \
    "https://github.com/DioxusLabs/dioxus/releases/download/v$version/$archive"
echo "$2  $archive" | (sha256sum -c - 2>/dev/null || shasum -a 256 -c -)

bin="$HOME/.cargo/bin"
mkdir -p "$bin"
case "$archive" in
*.zip) unzip -o "$archive" -d "$bin" ;;
*) tar -xzf "$archive" -C "$bin" dx ;;
esac
rm "$archive"
"$bin/dx" --version
