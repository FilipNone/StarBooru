#!/bin/sh
# Builds the Android app with the StarBooru launcher icon. dx writes its own default
# icons into the Gradle project on every build and has no setting for them, so this
# lets dx build, copies ours over its files, and runs Gradle again.
#   tools/brand/android-build.sh [--release] [--unsigned] [any other dx build flags]
# --unsigned, with --release, makes the APK to publish: see docs/development.md › Making
# a release. Plain --release makes one signed with the debug key, to try on a phone.
set -eu
cd "$(dirname "$0")/../.."

profile=debug
unsigned=
for arg in "$@"; do
    shift
    case "$arg" in
    --unsigned) unsigned=1 ;;
    --release) profile=release; set -- "$@" "$arg" ;;
    *) set -- "$@" "$arg" ;;
    esac
done
# dx signs and assembles a release build only with [bundle.android] configured.
task=assembleDebug
if [ "$profile" = release ] && { [ -n "$unsigned" ] || grep -q '^\[bundle.android\]' apps/app/Dioxus.toml; }; then
    task=assembleRelease
fi

dx=$(command -v dx || echo "$HOME/.cargo/bin/dx")
"$dx" build --android -p starbooru-app "$@"

project="target/dx/starbooru-app/$profile/android/app"
cp -R apps/app/android-res/. "$project/app/src/main/res/"
(cd "$project" && ./gradlew "$task")
find "$project/app/build/outputs/apk" -name '*.apk'
