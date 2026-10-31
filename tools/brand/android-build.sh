#!/bin/sh
# Builds the Android app with the StarBooru launcher icon. dx writes its own default
# icons into the Gradle project on every build and has no setting for them, so this
# lets dx build, copies ours over its files, and runs Gradle again.
#   tools/brand/android-build.sh [--release] [any other dx build flags]
set -eu
cd "$(dirname "$0")/../.."

profile=debug
for arg in "$@"; do
    if [ "$arg" = "--release" ]; then profile=release; fi
done
# dx signs and assembles a release build only with [bundle.android] configured.
task=assembleDebug
if [ "$profile" = release ] && grep -q '^\[bundle.android\]' apps/app/Dioxus.toml; then
    task=assembleRelease
fi

dx=$(command -v dx || echo "$HOME/.cargo/bin/dx")
"$dx" build --android -p starbooru-app "$@"

project="target/dx/starbooru-app/$profile/android/app"
cp -R apps/app/android-res/. "$project/app/src/main/res/"
(cd "$project" && ./gradlew "$task")
find "$project/app/build/outputs/apk" -name '*.apk'
