#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-all}"
OUTPUT_DIRECTORY="${2:-}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LAUNCHER_DIRECTORY="$REPO_ROOT/apps/launcher"
TAURI_DIRECTORY="$LAUNCHER_DIRECTORY/src-tauri"
RELEASE_DIRECTORY="$TAURI_DIRECTORY/target/release"

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

VERSION="$(node -p "require('$LAUNCHER_DIRECTORY/package.json').version")"
OS_NAME="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$OS_NAME" in
  darwin) PLATFORM="macos" ;;
  linux) PLATFORM="linux" ;;
  *)
    echo "Unsupported platform: $OS_NAME" >&2
    exit 1
    ;;
esac

copy_bundle_artifacts() {
  local bundle_name="$1"
  local filter="$2"
  local bundle_directory="$RELEASE_DIRECTORY/bundle/$bundle_name"

  if [[ ! -d "$bundle_directory" ]]; then
    echo "No $bundle_name artifact directory at $bundle_directory." >&2
    exit 1
  fi

  local matched=0
  local file
  shopt -s nullglob
  for file in "$bundle_directory"/$filter; do
    if [[ -f "$file" ]]; then
      cp -f "$file" "$OUTPUT_DIRECTORY/"
      matched=1
    fi
  done
  shopt -u nullglob

  if [[ "$matched" -eq 0 ]]; then
    echo "No $bundle_name artifact matching $filter in $bundle_directory." >&2
    exit 1
  fi
}

make_portable_archive() {
  local portable_archive="$OUTPUT_DIRECTORY/industrialis-launcher-$VERSION-$PLATFORM-portable.zip"

  if [[ "$PLATFORM" == "macos" ]]; then
    local app_path="$RELEASE_DIRECTORY/bundle/macos/industrialis-launcher.app"
    if [[ ! -d "$app_path" ]]; then
      echo "Portable app bundle was not produced at $app_path." >&2
      exit 1
    fi
    (
      cd "$RELEASE_DIRECTORY/bundle/macos"
      ditto -c -k --sequesterRsrc --keepParent "industrialis-launcher.app" "$portable_archive"
    )
    return
  fi

  local portable_executable="$RELEASE_DIRECTORY/industrialis-launcher"
  if [[ ! -f "$portable_executable" ]]; then
    echo "Portable executable was not produced at $portable_executable." >&2
    exit 1
  fi

  if command -v zip >/dev/null 2>&1; then
    (
      cd "$RELEASE_DIRECTORY"
      zip -q -9 "$portable_archive" "industrialis-launcher"
    )
  else
    # Fallback when zip is unavailable (rare on CI images).
    tar -C "$RELEASE_DIRECTORY" -a -cf "$portable_archive" "industrialis-launcher"
  fi
}

build_macos() {
  case "$TARGET" in
    all)
      pnpm tauri build --bundles dmg,app
      copy_bundle_artifacts "dmg" "*.dmg"
      make_portable_archive
      ;;
    installer)
      pnpm tauri build --bundles dmg
      copy_bundle_artifacts "dmg" "*.dmg"
      ;;
    portable)
      pnpm tauri build --bundles app
      make_portable_archive
      ;;
    *)
      echo "Unsupported target for macOS: $TARGET (use all, installer, or portable)" >&2
      exit 1
      ;;
  esac
}

build_linux() {
  case "$TARGET" in
    all)
      pnpm tauri build --no-bundle
      make_portable_archive
      pnpm tauri build --bundles deb,appimage
      copy_bundle_artifacts "deb" "*.deb"
      copy_bundle_artifacts "appimage" "*.AppImage"
      ;;
    installer)
      pnpm tauri build --bundles deb
      copy_bundle_artifacts "deb" "*.deb"
      ;;
    portable)
      # AppImage is the usual Linux portable package; also keep a plain binary zip.
      pnpm tauri build --no-bundle
      make_portable_archive
      pnpm tauri build --bundles appimage
      copy_bundle_artifacts "appimage" "*.AppImage"
      ;;
    *)
      echo "Unsupported target for Linux: $TARGET (use all, installer, or portable)" >&2
      exit 1
      ;;
  esac
}

pushd "$LAUNCHER_DIRECTORY" >/dev/null
case "$PLATFORM" in
  macos) build_macos ;;
  linux) build_linux ;;
esac
popd >/dev/null

echo "Launcher artifacts are available in $OUTPUT_DIRECTORY"
ls -lah "$OUTPUT_DIRECTORY"
