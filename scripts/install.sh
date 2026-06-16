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

download_status=""
download_error=""

curl_download() {
  url="$1"
  output="$2"

  rm -f "$download_error"
  if ! download_status="$(curl -sSL -w '%{http_code}' -o "$output" "$url" 2>"$download_error")"; then
    if [ -z "$download_status" ]; then
      download_status="000"
    fi
    return 1
  fi

  case "$download_status" in
    2??)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

print_download_error() {
  context="$1"
  url="$2"

  if [ "$download_status" = "000" ]; then
    echo "dx $action: $context: request failed: $url" >&2
  else
    echo "dx $action: $context: HTTP $download_status: $url" >&2
  fi

  if [ -s "$download_error" ]; then
    while IFS= read -r line; do
      echo "dx $action: curl: $line" >&2
    done <"$download_error"
  fi
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

target="$arch-$platform"
tmp_dir="$(mktemp -d)"
download_error="$tmp_dir/curl.err"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

if [ "$version" = "latest" ]; then
  latest_json="$tmp_dir/latest.json"
  latest_url="https://api.github.com/repos/$repo/releases/latest"
  if ! curl_download "$latest_url" "$latest_json"; then
    print_download_error "could not resolve latest release for $repo" "$latest_url"
    if [ "$download_status" = "404" ]; then
      echo "dx $action: no latest GitHub release exists for $repo" >&2
      echo "dx $action: create a GitHub release with dx-$target assets, or set DX_VERSION to an existing release" >&2
    fi
    exit 1
  fi

  tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$latest_json" | head -n 1)"
  if [ -z "$tag" ]; then
    echo "dx $action: could not resolve latest release for $repo: response did not contain tag_name" >&2
    exit 1
  fi
else
  case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
  esac
fi

package="dx-$tag-$target"
asset="$package.tar.gz"
base_url="https://github.com/$repo/releases/download/$tag"

cd "$tmp_dir"
if ! curl_download "$base_url/$asset" "$asset"; then
  print_download_error "could not download release asset $asset" "$base_url/$asset"
  echo "dx $action: release $tag for $repo must include $asset" >&2
  exit 1
fi
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
  print_download_error "could not download checksum $checksum" "$base_url/$checksum"
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
  echo "config: $binary config"
fi
