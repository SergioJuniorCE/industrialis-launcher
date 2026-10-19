#!/bin/sh
set -eu
test "$(uname -s)" = Linux && test -f /.dockerenv &&
    test "${GPUI_ISOLATED_EDITOR_CONTAINER:-}" = 1 || {
    echo 'Requires an explicitly owned disposable editor acceptance container' >&2; exit 2;
}
test "$#" -ge 2 || { echo 'Usage: test-gpui-linux-editor.sh <compiled-app> <output-dir> [probe|explorer]' >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=$(mktemp -d)
trap 'rm -f -- "$runtime/openbox.log"; rmdir "$runtime" 2>/dev/null || true' EXIT
chmod 700 "$runtime"
export XDG_RUNTIME_DIR=$runtime GPUI_ISOLATED_VIRTUAL_DESKTOP=1 PYTHONDONTWRITEBYTECODE=1
dbus-run-session -- xvfb-run -a -s '-screen 0 1800x1100x24' sh -c '
    openbox >"$XDG_RUNTIME_DIR/openbox.log" 2>&1 &
    shift
    if [ "${3:-}" = explorer ]; then
        exec python3 "$0/scripts/gpui-explorer-native-smoke.py" "$1" "$2"
    fi
    exec python3 "$0/scripts/gpui-editor-native-smoke.py" "$@"
' "$repo" _ "$@"
