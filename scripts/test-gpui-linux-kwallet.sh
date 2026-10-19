#!/bin/sh
# Native KWallet acceptance on a fresh, container-owned virtual desktop only.
set -eu
test "$(uname -s)" = Linux
test "${GPUI_ISOLATED_CREDENTIAL_CONTAINER:-}" = 1 || { echo 'Owned credential container required' >&2; exit 2; }
test -f /.dockerenv || { echo 'Disposable Docker container required' >&2; exit 2; }
test "$#" -eq 3 || { echo 'Usage: test-gpui-linux-kwallet.sh <electron> <rust-fixture> <receipt>' >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
owned=$(mktemp -d)
export HOME="$owned/home" XDG_RUNTIME_DIR="$owned/runtime" LANG=C.UTF-8
export XDG_CONFIG_HOME="$HOME/.config" XDG_DATA_HOME="$HOME/.local/share" TMPDIR="$HOME/tmp"
mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$TMPDIR"
chmod 700 "$owned" "$HOME" "$XDG_RUNTIME_DIR"
export GPUI_ACCEPTANCE_ELECTRON="$1" GPUI_ACCEPTANCE_RUST="$2" GPUI_CREDENTIAL_RECEIPT="$3"
export GPUI_ISOLATED_CREDENTIAL_SESSION=1 GPUI_ACCEPTANCE_PASSWORD_STORE=kwallet6
cat > "$XDG_CONFIG_HOME/kwalletrc" <<'EOF'
[Wallet]
Enabled=true
First Use=false
Use One Wallet=true
Default Wallet=kdewallet
Local Wallet=kdewallet
Close When Idle=false
Prompt on Open=false
EOF
dbus-run-session -- xvfb-run -a --server-args='-screen 0 1280x1024x24' sh -c '
  set -eu
  openbox > "$TMPDIR/wm.log" 2>&1 &
  kwalletd6 > "$TMPDIR/daemon.log" 2>&1 &
  daemon=$!
  sleep 2
  dbus-send --session --print-reply --reply-timeout=20000 --dest=org.kde.kwalletd6 /modules/kwalletd6 org.kde.KWallet.open string:kdewallet int64:0 string:GpuiDisposableProbe > "$TMPDIR/open.log" 2>&1 &
  opener=$!
  dialog=""
  for attempt in $(seq 1 100); do
    dialog=$(xdotool search --onlyvisible --pid "$daemon" --class "^kwalletd6$" 2>/dev/null | head -1 || true)
    test -z "$dialog" || break
    sleep 0.1
  done
  test -n "$dialog" || { echo "Owned KWallet creation dialog missing" >&2; exit 1; }
  test "$(xdotool getwindowpid "$dialog")" = "$daemon"
  xdotool windowactivate --sync "$dialog"
  # Observed native wizard on the fixed-size owned Xvfb screen. Select Classic
  # encryption, finish, then set a generated password in both masked fields.
  xdotool mousemove --window "$dialog" 120 142 click 1
  xdotool key alt+f
  sleep 1
  xdotool type --clearmodifiers "Generated disposable wallet"
  xdotool key Tab
  xdotool type --clearmodifiers "Generated disposable wallet"
  xdotool key Return
  wait "$opener"
  grep -Eq "int32 [0-9]+" "$TMPDIR/open.log" || { echo "Generated native wallet did not open" >&2; exit 1; }
  exec node --test "$1/scripts/gpui-credential-compat.test.mjs"
' sh "$repo"
