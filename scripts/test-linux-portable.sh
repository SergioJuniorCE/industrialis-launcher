#!/bin/sh
# Always creates its own virtual desktop and a fresh profile; never uses the user's display.
set -eu
test "$#" -eq 3 || { echo 'Usage: test-linux-portable.sh <archive> <compiled-binary> <output-dir>' >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=$(mktemp -d)
trap 'rm -f -- "$runtime/openbox.log"; rmdir "$runtime" 2>/dev/null || true' EXIT
chmod 700 "$runtime"
export XDG_RUNTIME_DIR="$runtime" LAUNCHER_ISOLATED_VIRTUAL_DESKTOP=1 PYTHONDONTWRITEBYTECODE=1
# Xvfb has no hardware GPU. Jammy's Mesa 23 lavapipe maps a Vulkan window but
# presents a black client; the same binary paints normally with Mesa OpenGL.
# Select that software backend explicitly for this owned virtual desktop only.
# Both names are needed for newer and older Vulkan loaders. Real desktop runs
# keep GPUI's default Vulkan/OpenGL selection; the portable PNG check stays strict.
export LIBGL_ALWAYS_SOFTWARE=1 VK_DRIVER_FILES=/dev/null VK_ICD_FILENAMES=/dev/null
dbus-run-session -- xvfb-run -a -s '-screen 0 1600x900x24' sh -c '
  openbox >"$XDG_RUNTIME_DIR/openbox.log" 2>&1 &
  exec python3 "$1/scripts/linux-portable-smoke.py" "$2" "$3" "$4"
' sh "$repo" "$1" "$2" "$3"
