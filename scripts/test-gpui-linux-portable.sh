#!/bin/sh
# Always creates its own virtual desktop and a fresh profile; never uses the user's display.
set -eu
test "$#" -eq 3 || { echo 'Usage: test-gpui-linux-portable.sh <archive> <compiled-binary> <output-dir>' >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=$(mktemp -d)
trap 'rm -f -- "$runtime/openbox.log"; rmdir "$runtime" 2>/dev/null || true' EXIT
chmod 700 "$runtime"
export XDG_RUNTIME_DIR="$runtime" GPUI_ISOLATED_VIRTUAL_DESKTOP=1 PYTHONDONTWRITEBYTECODE=1
dbus-run-session -- xvfb-run -a -s '-screen 0 1600x900x24' sh -c '
  openbox >"$XDG_RUNTIME_DIR/openbox.log" 2>&1 &
  exec python3 "$1/scripts/gpui-linux-portable-smoke.py" "$2" "$3" "$4"
' sh "$repo" "$1" "$2" "$3"
