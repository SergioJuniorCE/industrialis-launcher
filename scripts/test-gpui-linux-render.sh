#!/bin/sh
# Synthetic instances only. Uses a virtual desktop, never the user's display.
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "${GPUI_APP_DIR:-$repo_dir/apps/launcher}"
cargo build -p industrialis-app --example window_move_bench --jobs 1 --locked
target_dir=${CARGO_TARGET_DIR:-target}
benchmark=$(CDPATH= cd -- "$target_dir/debug/examples" && pwd)/window_move_bench
runtime_dir=$(mktemp -d)
trap 'rmdir "$runtime_dir" 2>/dev/null || true' EXIT
export XDG_RUNTIME_DIR=$runtime_dir
# Cloud/VM software rendering is a startup/layout smoke, not a desktop FPS gate.
export GPUI_BENCH_BUDGET_MS=${GPUI_BENCH_BUDGET_MS:-1000}
dbus-run-session -- xvfb-run -a -s '-screen 0 1600x900x24' sh -c '
  openbox >/tmp/industrialis-gpui-openbox.log 2>&1 &
  exec timeout 45 "$1"
' sh "$benchmark"
