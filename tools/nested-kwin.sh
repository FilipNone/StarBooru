#!/usr/bin/env bash
# Runs a command inside a throwaway headless KWin that cannot touch the desktop session.
#
#   tools/nested-kwin.sh [options] -- command [args...]
#
#   --socket NAME         Wayland socket name (default: sbtest-<pid>); refused if it already exists
#   --width N, --height N size of the virtual output (default: 1280x800)
#   --x-display :N=WxH    also start a rootful Xwayland, repeatable; the first becomes DISPLAY
#                         (xdotool and `import -window root` need one)
#   --timeout SECONDS     stop everything after this long (default: 600)
#   --log FILE            keep KWin's log here (default: deleted with the temporary directory)
#
# The command's exit status is returned. 124 means timeout, 129/130/143 a signal, 2 bad usage,
# 3 the isolation could not be set up, 4 a process survived cleanup.
#
# Isolation (why this exists is in docs/development.md, "Nested KWin"):
# - dbus-run-session gives KWin and the command a private session bus. On the desktop's bus a nested
#   KWin registers the same "kwin" shortcut component as the real one, and its exit deactivates it.
# - XDG_CONFIG_HOME, XDG_DATA_HOME, XDG_CACHE_HOME and XDG_STATE_HOME point at a temporary directory,
#   so the desktop's kwinrc, kwinoutputconfig.json and kglobalshortcutsrc are never written.
# - XDG_RUNTIME_DIR stays as it is: it only holds the uniquely named socket. --virtual needs no host
#   display, so WAYLAND_DISPLAY, DISPLAY and XAUTHORITY are dropped.
#
# Cleanup only signals processes in the harness's own session or carrying its random id in their
# environment. It never looks processes up by name.
set -euo pipefail

die() {
  echo "nested-kwin: $*" >&2
  exit 2
}

# Runs as the session command of the nested KWin, on the private bus.
session_stage() {
  local kwin=$PPID dir=$NESTED_KWIN_DIR host_pid=$NESTED_KWIN_HOST_PID
  local owner='' spec display geometry first=1 status
  local -a xservers=()

  # KWin's output goes to its log; give the command the harness's real stdout and stderr.
  if (exec >>"/proc/$host_pid/fd/1") 2>/dev/null; then exec >>"/proc/$host_pid/fd/1"; fi
  if (exec 2>>"/proc/$host_pid/fd/2") 2>/dev/null; then exec 2>>"/proc/$host_pid/fd/2"; fi

  # Skipping KWin's shutdown is deliberate: a clean exit is what deactivates shortcuts on a shared bus.
  fail() {
    echo "nested-kwin: $*" >&2
    echo 3 >"$dir/status"
    kill -KILL "$kwin" 2>/dev/null || true
    exit 3
  }

  echo "$kwin" >"$dir/kwin.pid"
  [[ $(cat "/proc/$kwin/comm" 2>/dev/null) == kwin_wayland ]] || fail "parent is not the nested KWin"
  [[ -n ${DBUS_SESSION_BUS_ADDRESS:-} && $DBUS_SESSION_BUS_ADDRESS != "$NESTED_KWIN_HOST_BUS" ]] ||
    fail "not on a private session bus"
  [[ $XDG_CONFIG_HOME == "$dir"/* && $XDG_DATA_HOME == "$dir"/* && $XDG_CACHE_HOME == "$dir"/* && $XDG_STATE_HOME == "$dir"/* ]] ||
    fail "XDG directories are not isolated"

  # The environment says what was asked for; this says KWin really is on the private bus.
  for _ in {1..50}; do
    owner=$(dbus-send --session --print-reply=literal --dest=org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus.GetConnectionUnixProcessID string:org.kde.kglobalaccel 2>/dev/null) || owner=''
    owner=${owner##* }
    [[ $owner == "$kwin" ]] && break
    sleep 0.2
  done
  [[ $owner == "$kwin" ]] || fail "KWin does not own org.kde.kglobalaccel on the private bus (owner: ${owner:-none})"

  # If the harness itself gets SIGKILLed, nothing else would stop this tree.
  (
    trap '' TERM INT HUP
    while kill -0 "$host_pid" 2>/dev/null; do sleep 1; done
    kill -TERM 0
    sleep 3
    rm -rf -- "$dir"
    kill -KILL 0
  ) </dev/null >/dev/null 2>&1 &
  local watchdog=$!

  export WAYLAND_DISPLAY=$NESTED_KWIN_SOCKET
  for spec in $NESTED_KWIN_X; do
    display=${spec%%=*}
    geometry=${spec#*=}
    Xwayland "$display" -geometry "$geometry" -noreset -ac >>"$NESTED_KWIN_LOG" 2>&1 &
    xservers+=("$!")
    for _ in {1..50}; do
      [[ -S /tmp/.X11-unix/X${display#:} ]] && break
      sleep 0.2
    done
    [[ -S /tmp/.X11-unix/X${display#:} ]] || fail "Xwayland $display did not start"
    if ((first)); then
      export DISPLAY=$display
      first=0
    fi
  done

  export STARBOORU_NESTED_KWIN=1
  status=0
  "$@" || status=$?
  echo "$status" >"$dir/status"
  kill "$watchdog" "${xservers[@]}" 2>/dev/null || true
  exit "$status"
}

if [[ ${1:-} == --session-stage ]]; then
  shift
  session_stage "$@"
fi

socket="sbtest-$$"
width=1280
height=800
limit=600
log=''
x_specs=()
while (($#)); do
  case $1 in
    --socket) socket=${2:?}; shift 2 ;;
    --width) width=${2:?}; shift 2 ;;
    --height) height=${2:?}; shift 2 ;;
    --x-display) x_specs+=("${2:?}"); shift 2 ;;
    --timeout) limit=${2:?}; shift 2 ;;
    --log) log=${2:?}; shift 2 ;;
    --) shift; break ;;
    *) die "unknown option $1 (usage is at the top of this script)" ;;
  esac
done
(($#)) || die "no command given (usage is at the top of this script)"

[[ $socket =~ ^[A-Za-z0-9._-]+$ ]] || die "bad socket name: $socket"
for tool in dbus-run-session dbus-send kwin_wayland setsid pgrep; do
  command -v "$tool" >/dev/null || die "$tool is not installed"
done
((${#x_specs[@]} == 0)) || command -v Xwayland >/dev/null || die "Xwayland is not installed"

runtime=${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is not set}
[[ ! -e $runtime/$socket && ! -e $runtime/$socket.lock ]] || die "socket $socket is already in use"
x_numbers=()
for spec in ${x_specs[@]+"${x_specs[@]}"}; do
  [[ $spec =~ ^:([0-9]+)=[0-9]+x[0-9]+$ ]] || die "bad --x-display $spec (expected :N=WxH)"
  x_numbers+=("${BASH_REMATCH[1]}")
  [[ ! -e /tmp/.X11-unix/X${BASH_REMATCH[1]} && ! -e /tmp/.X${BASH_REMATCH[1]}-lock ]] ||
    die "X display ${spec%%=*} is already in use"
done

host_bus=${DBUS_SESSION_BUS_ADDRESS:-unix:path=$runtime/bus}
base=$(mktemp -d "${TMPDIR:-/tmp}/nested-kwin.XXXXXXXXXX")
# KWin splits --exit-with-session on spaces, so the path to session.sh can't contain any.
if [[ $base == *[[:space:]]* ]]; then
  rmdir -- "$base"
  die "the temporary directory path contains whitespace: $base"
fi
id=${base##*/}
mkdir "$base/config" "$base/data" "$base/cache" "$base/state"
log=${log:-$base/kwin.log}
child=''
timer=''

# Everything this run started and that is still running. Both lookups are specific to this run.
harness_pids() {
  local proc pid
  {
    [[ -z $child ]] || pgrep -s "$child" || true
    for proc in /proc/[0-9]*; do
      grep -qaz "^NESTED_KWIN_ID=$id\$" "$proc/environ" 2>/dev/null && echo "${proc#/proc/}"
    done
  } | sort -un | while read -r pid; do
    [[ $pid != "$$" && $(ps -o stat= -p "$pid" 2>/dev/null) != Z* ]] && echo "$pid"
  done
  return 0
}

cleanup() {
  local status=$? pids sig kwin number
  trap - EXIT INT TERM HUP
  [[ -z $timer ]] || kill "$timer" 2>/dev/null || true
  pids=$(harness_pids)
  for sig in TERM KILL; do
    [[ -n $pids ]] || break
    # shellcheck disable=SC2086 # one pid per word
    kill -"$sig" $pids 2>/dev/null || true
    for _ in {1..25}; do
      sleep 0.2
      pids=$(harness_pids)
      [[ -n $pids ]] || break
    done
  done
  if [[ -n $pids ]]; then
    echo "nested-kwin: still running after cleanup: $(echo $pids)" >&2
    status=4
  fi

  # A compositor stopped with SIGKILL leaves these behind. All of them are names this run claimed.
  rm -f -- "$runtime/$socket" "$runtime/$socket.lock"
  kwin=$(cat "$base/kwin.pid" 2>/dev/null || true)
  [[ -z $kwin ]] || rm -f -- "$runtime/kwin-xwayland-eis-socket.$kwin" "$runtime/kwin-xwayland-eis-socket.$kwin.lock"
  for number in ${x_numbers[@]+"${x_numbers[@]}"}; do
    rm -f -- "/tmp/.X11-unix/X$number" "/tmp/.X$number-lock"
  done
  rm -rf -- "$base"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

self=$(readlink -f -- "${BASH_SOURCE[0]}")
{
  echo '#!/usr/bin/env bash'
  printf 'exec bash %q --session-stage' "$self"
  printf ' %q' "$@"
  echo
} >"$base/session.sh"
chmod +x "$base/session.sh"

# Backgrounded without job control, so setsid doesn't fork and $! is the new session's id.
# Ctrl-C reaches only this script, which then stops the whole tree in order.
setsid env \
  -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u DISPLAY -u XAUTHORITY \
  -u SESSION_MANAGER -u ICEAUTHORITY -u NOTIFY_SOCKET -u KDE_APPLICATIONS_AS_SCOPE \
  -u XDG_ACTIVATION_TOKEN -u DESKTOP_STARTUP_ID \
  XDG_CONFIG_HOME="$base/config" XDG_DATA_HOME="$base/data" \
  XDG_CACHE_HOME="$base/cache" XDG_STATE_HOME="$base/state" XDG_CONFIG_DIRS=/etc/xdg \
  NESTED_KWIN_ID="$id" NESTED_KWIN_DIR="$base" NESTED_KWIN_HOST_PID="$$" \
  NESTED_KWIN_HOST_BUS="$host_bus" NESTED_KWIN_SOCKET="$socket" NESTED_KWIN_LOG="$log" \
  NESTED_KWIN_X="${x_specs[*]-}" \
  dbus-run-session -- kwin_wayland --virtual --xwayland --no-lockscreen --socket "$socket" \
  --width "$width" --height "$height" --exit-with-session "$base/session.sh" \
  </dev/null >>"$log" 2>&1 &
child=$!

sleep "$limit" &
timer=$!
finished=''
wait -n -p finished "$child" "$timer" || true
if [[ $finished == "$timer" ]]; then
  timer=''
  echo "nested-kwin: timed out after ${limit}s" >&2
  exit 124
fi

if [[ ! -f $base/status ]]; then
  echo "nested-kwin: KWin exited before the command finished. Last lines of its log:" >&2
  tail -n 20 -- "$log" >&2 || true
  exit 3
fi
exit "$(cat "$base/status")"
