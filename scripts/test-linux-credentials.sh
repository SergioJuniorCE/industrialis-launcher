#!/bin/sh
# Fresh container-only HOME, bus and keyring. Never connects to a desktop keyring.
set -eu
test "$(uname -s)" = Linux
test "${LAUNCHER_ISOLATED_CREDENTIAL_CONTAINER:-}" = 1 || { echo 'Owned credential container required' >&2; exit 2; }
test -f /.dockerenv || { echo 'Disposable Docker container required' >&2; exit 2; }
test "$#" -eq 3 || { echo 'Usage: test-linux-credentials.sh <electron> <rust-fixture> <receipt>' >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
owned=$(mktemp -d)
export HOME="$owned/home" XDG_RUNTIME_DIR="$owned/runtime"
export XDG_CONFIG_HOME="$HOME/.config" XDG_DATA_HOME="$HOME/.local/share" TMPDIR="$HOME/tmp"
mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$TMPDIR" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
chmod 700 "$owned" "$HOME" "$XDG_RUNTIME_DIR"
export LAUNCHER_ACCEPTANCE_ELECTRON="$1" LAUNCHER_ACCEPTANCE_RUST="$2" LAUNCHER_CREDENTIAL_RECEIPT="$3"
export LAUNCHER_ISOLATED_CREDENTIAL_SESSION=1 LAUNCHER_ACCEPTANCE_PASSWORD_STORE=gnome-libsecret
dbus-run-session -- xvfb-run -a sh -c '
  printf "%s\n" "Generated expendable acceptance keyring" | gnome-keyring-daemon --unlock --components=secrets
  exec node --test "$1/scripts/credential-compat.test.mjs"
' sh "$repo"
