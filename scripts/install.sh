#!/usr/bin/env sh
set -eu

repo="${DX_REPO:-phongndo/dx}"
version="${DX_VERSION:-latest}"
install_dir="${DX_INSTALL_DIR:-$HOME/.local/bin}"
binary="${DX_BINARY:-dx}"
action="${DX_INSTALL_ACTION:-install}"
case "$action" in
  install | update)
    ;;
  *)
    action="install"
    ;;
esac

curl_download() {
  curl -fsSL "$1" -o "$2"
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "dx $action: missing required command: $1" >&2
    exit 1
  fi
}

allow_unverified() {
  case "${DX_ALLOW_UNVERIFIED:-}" in
    1 | [Tt][Rr][Uu][Ee] | [Yy][Ee][Ss])
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

need curl
need tar
need install

case "$(uname -s)" in
  Darwin)
    platform="apple-darwin"
    ;;
  Linux)
    platform="unknown-linux-gnu"
    ;;
  *)
    echo "dx $action: unsupported OS: $(uname -s)" >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  arm64 | aarch64)
    arch="aarch64"
    ;;
  x86_64 | amd64)
    arch="x86_64"
    ;;
  *)
    echo "dx $action: unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if [ "$version" = "latest" ]; then
  tag="$(
    curl -fsSL "https://api.github.com/repos/$repo/releases/latest" \
      | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n 1
  )"
  if [ -z "$tag" ]; then
    echo "dx $action: could not resolve latest release for $repo" >&2
    exit 1
  fi
else
  case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
  esac
fi

target="$arch-$platform"
package="dx-$tag-$target"
asset="$package.tar.gz"
base_url="https://github.com/$repo/releases/download/$tag"
tmp_dir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

cd "$tmp_dir"
curl_download "$base_url/$asset" "$asset"
checksum="$asset.sha256"
if curl_download "$base_url/$checksum" "$checksum"; then
  if command -v shasum >/dev/null 2>&1; then
    if [ "$action" = "update" ]; then
      shasum -a 256 -c "$checksum" >/dev/null
    else
      shasum -a 256 -c "$checksum"
    fi
  elif command -v sha256sum >/dev/null 2>&1; then
    if [ "$action" = "update" ]; then
      sha256sum -c "$checksum" >/dev/null
    else
      sha256sum -c "$checksum"
    fi
  elif allow_unverified; then
    echo "dx $action: warning: shasum or sha256sum not found; skipping checksum verification" >&2
  else
    echo "dx $action: shasum or sha256sum not found; set DX_ALLOW_UNVERIFIED=1 to skip checksum verification" >&2
    exit 1
  fi
elif allow_unverified; then
  echo "dx $action: warning: checksum file not available; skipping checksum verification" >&2
else
  echo "dx $action: checksum file not available; set DX_ALLOW_UNVERIFIED=1 to skip checksum verification" >&2
  exit 1
fi

tar -xzf "$asset"
install_source="$package/dx"
if [ ! -d "$package" ] || [ ! -x "$install_source" ]; then
  echo "dx $action: extracted archive does not contain executable $install_source" >&2
  exit 1
fi

mkdir -p "$install_dir"
install -m 755 "$install_source" "$install_dir/$binary"

if [ "$action" = "update" ]; then
  echo "updated $binary to $tag at $install_dir/$binary"
else
  echo "installed $binary $tag to $install_dir/$binary"
  echo "run: $binary --version"
fi
