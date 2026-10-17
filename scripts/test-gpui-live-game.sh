#!/bin/sh
# Opt-in real client acceptance, using a marked expendable profile and Xvfb.
set -eu
if [ "$#" -ne 3 ] && [ "$#" -ne 4 ]; then
  echo 'Usage: test-gpui-live-game.sh <expendable-profile> <instance-id> <java-path> [--detached|--microsoft|--normal-exit|--abnormal-exit|--server-world]' >&2
  exit 2
fi
example=live_game_acceptance
if [ "$#" -eq 4 ]; then
  case "$4" in
    --detached) example=live_detached_game_acceptance ;;
    --microsoft) example=live_provider_acceptance ;;
    --normal-exit) example=live_connection_exit_acceptance; export GPUI_GAME_EXIT_MODE=normal ;;
    --abnormal-exit) example=live_connection_exit_acceptance; export GPUI_GAME_EXIT_MODE=abnormal ;;
    --server-world) example=live_connection_exit_acceptance; export GPUI_GAME_EXIT_MODE=server-world ;;
    *) echo 'Unknown acceptance mode' >&2; exit 2 ;;
  esac
fi
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app=${GPUI_APP_DIR:-"$repo/apps/launcher-gpui"}
if [ -n "${GPUI_LIVE_GAME_BINARY:-}" ]; then
  binary=$GPUI_LIVE_GAME_BINARY
else
  cargo build --manifest-path "$app/Cargo.toml" -p industrialis-backend --example "$example" --jobs 1 --locked
  binary=${CARGO_TARGET_DIR:-"$app/target"}/debug/examples/$example
fi
desktop_dir=$(mktemp -d)
trap 'rm -rf -- "$desktop_dir"' EXIT HUP INT TERM
mkdir -p "$desktop_dir/data/applications" "$desktop_dir/runtime"
chmod 700 "$desktop_dir/runtime"
export XDG_DATA_HOME="$desktop_dir/data" XDG_RUNTIME_DIR="$desktop_dir/runtime"
export LIBGL_ALWAYS_SOFTWARE=${LIBGL_ALWAYS_SOFTWARE:-1}
export GPUI_ISOLATED_VIRTUAL_DESKTOP=1
export GPUI_VIRTUAL_WINDOW_HELPER="$repo/scripts/gpui-virtual-game-window.py"
if [ "$example" = live_provider_acceptance ]; then
  dbus-run-session -- xvfb-run -a -s '-screen 0 1280x720x24' "$binary" "$1" microsoft-game "$2" "$3"
else
  dbus-run-session -- xvfb-run -a -s '-screen 0 1280x720x24' "$binary" "$1" "$2" "$3"
fi
