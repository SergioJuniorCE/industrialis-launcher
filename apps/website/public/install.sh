#!/usr/bin/env bash
# Industrialis server CLI installer
# Usage:
#   curl -fsSL https://industrialislauncher.yoggan.dev/install.sh | bash
# Or inspect first:
#   curl -fsSL https://industrialislauncher.yoggan.dev/install.sh -o install.sh
#   less install.sh && bash install.sh
set -euo pipefail

REPO="${INDUSTRIALIS_REPO:-SergioJuniorCE/industrialis-launcher}"
BASE_URL="${INDUSTRIALIS_RELEASE_BASE:-https://github.com/${REPO}/releases}"
INSTALL_DIR="${INDUSTRIALIS_INSTALL_DIR:-${HOME}/.local/share/industrialis}"
BIN_DIR="${INDUSTRIALIS_BIN_DIR:-${HOME}/.local/bin}"
VERSION="${INDUSTRIALIS_VERSION:-latest}"

info() { printf '==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

detect_arch() {
  local arch
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) echo "x64" ;;
    *) die "unsupported architecture: $arch (need x86_64)" ;;
  esac
}

detect_os() {
  local os
  os="$(uname -s)"
  case "$os" in
    Linux) echo "linux" ;;
    *) die "unsupported OS: $os (Linux only for now)" ;;
  esac
}

check_node() {
  require_cmd node
  local major
  major="$(node -p "process.versions.node.split('.')[0]")"
  if [ "$major" -lt 22 ]; then
    die "Node.js 22+ is required (found $(node -v))"
  fi
}

check_docker() {
  require_cmd docker
  if ! docker info >/dev/null 2>&1; then
    die "Docker Engine is not reachable. Install Docker and ensure this user can access the socket."
  fi
}

resolve_download_url() {
  local os arch asset release_tag
  os="$(detect_os)"
  arch="$(detect_arch)"
  asset="industrialis-server-${os}-${arch}.tar.gz"

  if [ "$VERSION" = "latest" ]; then
    if [ "$BASE_URL" = "https://github.com/${REPO}/releases" ]; then
      if ! release_tag="$(curl -fsSL \
        -H 'Accept: application/vnd.github+json' \
        "https://api.github.com/repos/${REPO}/releases?per_page=100" | \
        node -e '
          const releases = JSON.parse(require("fs").readFileSync(0, "utf8"));
          const release = releases.find((item) =>
            !item.draft && !item.prerelease && /^server-v/.test(item.tag_name),
          );
          process.stdout.write(release?.tag_name ?? "");
        ')"; then
        die "could not resolve the latest server release from GitHub"
      fi
      [ -n "$release_tag" ] || die "no published server release was found"
      echo "${BASE_URL}/download/${release_tag}/${asset}"
    else
      echo "${BASE_URL}/latest/download/${asset}"
    fi
  else
    echo "${BASE_URL}/download/${VERSION}/${asset}"
  fi
}

main() {
  require_cmd curl
  require_cmd tar
  check_node
  check_docker

  local url tmp archive
  url="$(resolve_download_url)"
  tmp="$(mktemp -d)"
  archive="${tmp}/industrialis-server.tar.gz"

  info "Downloading ${url}"
  if ! curl -fsSL "$url" -o "$archive"; then
    die "download failed. Publish a server release asset or set INDUSTRIALIS_RELEASE_BASE / INDUSTRIALIS_VERSION."
  fi

  info "Installing to ${INSTALL_DIR}"
  rm -rf "${INSTALL_DIR}"
  mkdir -p "${INSTALL_DIR}" "${BIN_DIR}"
  tar -xzf "$archive" -C "$tmp"
  # Support either a top-level folder or flat archive contents.
  if [ -d "${tmp}/industrialis" ]; then
    cp -a "${tmp}/industrialis/." "${INSTALL_DIR}/"
  else
    # Copy everything except the archive itself.
    find "$tmp" -mindepth 1 -maxdepth 1 ! -name 'industrialis-server.tar.gz' -exec cp -a {} "${INSTALL_DIR}/" \;
  fi

  if [ ! -f "${INSTALL_DIR}/bin/industrialis" ] && [ -f "${INSTALL_DIR}/dist/cli.js" ]; then
    mkdir -p "${INSTALL_DIR}/bin"
    cat > "${INSTALL_DIR}/bin/industrialis" <<EOF
#!/usr/bin/env bash
exec node "${INSTALL_DIR}/dist/cli.js" "\$@"
EOF
    chmod +x "${INSTALL_DIR}/bin/industrialis"
  fi

  chmod +x "${INSTALL_DIR}/bin/industrialis" 2>/dev/null || true
  ln -sfn "${INSTALL_DIR}/bin/industrialis" "${BIN_DIR}/industrialis"

  rm -rf "$tmp"

  info "Installed industrialis -> ${BIN_DIR}/industrialis"
  if ! command -v industrialis >/dev/null 2>&1; then
    printf '\nAdd %s to your PATH, then re-open the shell:\n' "${BIN_DIR}"
    printf '  export PATH="%s:\$PATH"\n\n' "${BIN_DIR}"
  fi

  cat <<'EOF'

Next steps:
  industrialis up        # start daemon + dashboard
  industrialis status
  industrialis down

Dashboard defaults to http://127.0.0.1:3001 (loopback only).
EOF
}

main "$@"
