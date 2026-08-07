#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-all}"
OUTPUT_DIRECTORY="${2:-}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LAUNCHER_DIRECTORY="$REPO_ROOT/apps/launcher"
MAKE_DIRECTORY="$LAUNCHER_DIRECTORY/out/make"

if [[ -z "$OUTPUT_DIRECTORY" ]]; then
  OUTPUT_DIRECTORY="$REPO_ROOT/artifacts/launcher"
elif [[ "$OUTPUT_DIRECTORY" != /* ]]; then
  OUTPUT_DIRECTORY="$REPO_ROOT/$OUTPUT_DIRECTORY"
fi

if ! command -v pnpm >/dev/null 2>&1; then
  echo "pnpm is required but was not found on PATH." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIRECTORY"

copy_make_artifacts() {
  if [[ ! -d "$MAKE_DIRECTORY" ]]; then
    echo "Electron Forge did not produce artifacts in $MAKE_DIRECTORY." >&2
    exit 1
  fi
  find "$MAKE_DIRECTORY" -type f -exec cp -f {} "$OUTPUT_DIRECTORY/" \;
}

make_portable_archive() {
  local version
  version="$(node -p "require('$LAUNCHER_DIRECTORY/package.json').version")"
  local package_directory="$LAUNCHER_DIRECTORY/out/industrialis-launcher-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    package_directory="$LAUNCHER_DIRECTORY/out/industrialis-launcher-darwin-arm64"
    [[ -d "$package_directory" ]] || package_directory="$LAUNCHER_DIRECTORY/out/industrialis-launcher-darwin-x64"
  elif [[ "$(uname -s)" == "Linux" ]]; then
    package_directory="$LAUNCHER_DIRECTORY/out/industrialis-launcher-linux-x64"
  fi
  if [[ ! -d "$package_directory" ]]; then
    echo "Electron package was not produced at $package_directory." >&2
    exit 1
  fi
  local portable_archive="$OUTPUT_DIRECTORY/industrialis-launcher-$version-$(uname -s | tr '[:upper:]' '[:lower:]')-portable.zip"
  (cd "$package_directory" && zip -q -9 -r "$portable_archive" .)
}

pushd "$LAUNCHER_DIRECTORY" >/dev/null
case "$TARGET" in
  portable)
    pnpm package
    make_portable_archive
    ;;
  all)
    pnpm make
    copy_make_artifacts
    make_portable_archive
    ;;
  installer)
    pnpm make
    copy_make_artifacts
    ;;
  *)
    echo "Unsupported launcher target: $TARGET (use all, installer, or portable)." >&2
    exit 1
    ;;
esac
popd >/dev/null

echo "Launcher artifacts are available in $OUTPUT_DIRECTORY"
ls -lah "$OUTPUT_DIRECTORY"
