#!/bin/bash
# Starts the desktop build on a CI runner and fails if it dies within 25 seconds.
# Leaves smoke.png for a person to look at; a missing screenshot fails nothing.
set -u

screenshot() {
    case "$(uname -s)" in
    Linux) import -window root smoke.png ;;
    Darwin) screencapture -x smoke.png ;;
    *) powershell -NoProfile -Command '
        Add-Type -AssemblyName System.Windows.Forms,System.Drawing
        $b = [Windows.Forms.Screen]::PrimaryScreen.Bounds
        $i = New-Object Drawing.Bitmap $b.Width,$b.Height
        [Drawing.Graphics]::FromImage($i).CopyFromScreen($b.Location,[Drawing.Point]::Empty,$b.Size)
        $i.Save("smoke.png")' ;;
    esac || echo "no screenshot"
}

release=target/dx/starbooru-app/release
case "$(uname -s)" in
Linux) exe=$(find "$release/linux" -type f -executable -name 'starbooru*' | head -n 1) ;;
Darwin) exe=$(find "$release/macos" -type f -path '*/Contents/MacOS/*' | head -n 1) ;;
*) exe=$(find "$release/windows" -type f -name '*.exe' | head -n 1) ;;
esac
if [ -z "$exe" ]; then
    echo "no app found under $release"
    find "$release" -maxdepth 4
    exit 1
fi
echo "starting $exe"

if [ "$(uname -s)" = Linux ]; then
    Xvfb :99 -screen 0 1280x800x24 &
    export DISPLAY=:99
    sleep 2
fi
"$exe" &
pid=$!
sleep 25
screenshot
if ! kill -0 "$pid" 2>/dev/null; then
    wait "$pid"
    echo "the app exited early with code $?"
    exit 1
fi
kill "$pid" 2>/dev/null
echo "still running after 25 seconds"
