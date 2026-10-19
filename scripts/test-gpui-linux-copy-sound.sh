#!/bin/sh
# All input and bell observation stay on a newly owned container/Xvfb desktop.
set -eu
test "$(uname -s)" = Linux && test -f /.dockerenv &&
    test "${GPUI_ISOLATED_SOUND_CONTAINER:-}" = 1 || {
    echo 'Requires an explicitly owned disposable Linux container' >&2; exit 2;
}
test "$#" -eq 2 || {
    echo 'Usage: test-gpui-linux-copy-sound.sh <compiled-app> <output-dir>' >&2; exit 2;
}
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=$(mktemp -d)
trap 'rm -f -- "$runtime/openbox.log"; rmdir "$runtime" 2>/dev/null || true' EXIT
chmod 700 "$runtime"
export XDG_RUNTIME_DIR=$runtime GPUI_ISOLATED_VIRTUAL_DESKTOP=1 PYTHONDONTWRITEBYTECODE=1
dbus-run-session -- xvfb-run -a -s '-screen 0 1600x900x24' sh -c '
    openbox >"$XDG_RUNTIME_DIR/openbox.log" 2>&1 &
    exec python3 "$1/scripts/gpui-copy-sound-smoke.py" "$2" "$3"
' sh "$repo" "$1" "$2"
