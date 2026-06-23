#!/usr/bin/env sh
set -eu

repo="${MARK_REPO:-phongndo/mark}"
version="${MARK_VERSION:-latest}"
install_dir="${MARK_INSTALL_DIR:-$HOME/.local/bin}"
binary="${MARK_BINARY:-mark}"
action="${MARK_INSTALL_ACTION:-install}"
case "$action" in
  install | update)
    ;;
  *)
    action="install"
    ;;
esac

print_plan() {
  if [ "$action" = "update" ]; then
    printf 'Updating %s\n' "$binary"
    if [ -n "${MARK_CURRENT_VERSION:-}" ]; then
      printf '  from: v%s\n' "${MARK_CURRENT_VERSION#v}"
    fi
    printf '  to:   %s\n' "$tag"
    printf '  path: %s\n' "$install_dir/$binary"
  else
    printf 'Installing %s\n' "$binary"
    printf '  version: %s\n' "$tag"
    printf '  target:  %s\n' "$target"
    printf '  path:    %s\n' "$install_dir/$binary"
  fi
  printf '\n'
}

print_success() {
  printf '✓ %s\n' "$1"
}

print_path_hint() {
  active_binary="$(command -v "$binary" 2>/dev/null || true)"
  if [ "$active_binary" = "$install_dir/$binary" ]; then
    return 0
  fi

  printf '\n%s was installed, but your shell may not find it yet.\n' "$binary"
  printf 'Add this to your shell profile:\n\n'
  printf '  export PATH="%s:$PATH"\n' "$install_dir"
}

managed_install_name() {
  case "$1" in
    /opt/homebrew/* | /home/linuxbrew/.linuxbrew/* | */.linuxbrew/* | */Cellar/*)
      printf 'Homebrew'
      return 0
      ;;
    */.cargo/bin | */.cargo/bin/*)
      printf 'Cargo'
      return 0
      ;;
    */.local/share/mise/shims | */.local/share/mise/shims/* | */.local/share/mise/installs | */.local/share/mise/installs/* | */.mise/shims | */.mise/shims/* | */.mise/installs | */.mise/installs/*)
      printf 'mise'
      return 0
      ;;
    /nix/store/* | */.nix-profile/bin | */.nix-profile/bin/* | */.local/state/nix/profile/bin | */.local/state/nix/profile/bin/* | /run/current-system/sw/bin | /run/current-system/sw/bin/*)
      printf 'Nix'
      return 0
      ;;
    */.asdf/shims | */.asdf/shims/* | */.asdf/installs | */.asdf/installs/*)
      printf 'asdf'
      return 0
      ;;
  esac

  return 1
}

refuse_managed_path() {
  path="$1"
  label="$2"
  manager="$(managed_install_name "$path" || true)"
  if [ -z "$manager" ]; then
    return 0
  fi

  echo "mark $action: refusing to write to $manager-managed $label: $path" >&2
  echo "mark $action: choose an unmanaged directory, for example: $HOME/.local/bin" >&2
  exit 1
}

refuse_managed_install_dir() {
  install_target="$install_dir/$binary"

  refuse_managed_path "$install_dir" "install directory"
  refuse_managed_path "$install_target" "install target"

  if [ -L "$install_target" ] && command -v readlink >/dev/null 2>&1; then
    link_target="$(readlink "$install_target" || true)"
    if [ -n "$link_target" ]; then
      case "$link_target" in
        /*) resolved_target="$link_target" ;;
        *) resolved_target="$install_dir/$link_target" ;;
      esac
      refuse_managed_path "$resolved_target" "install target symlink"
    fi
  fi

  if [ -e "$install_target" ] && command -v realpath >/dev/null 2>&1; then
    real_target="$(realpath "$install_target" 2>/dev/null || true)"
    if [ -n "$real_target" ]; then
      refuse_managed_path "$real_target" "install target"
    fi
  fi
}

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
    echo "mark $action: $context: request failed: $url" >&2
  else
    echo "mark $action: $context: HTTP $download_status: $url" >&2
  fi

  if [ -s "$download_error" ]; then
    while IFS= read -r line; do
      echo "mark $action: curl: $line" >&2
    done <"$download_error"
  fi
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "mark $action: missing required command: $1" >&2
    exit 1
  fi
}

allow_unverified() {
  case "${MARK_ALLOW_UNVERIFIED:-}" in
    1 | [Tt][Rr][Uu][Ee] | [Yy][Ee][Ss])
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

is_mark_release_tag() {
  case "$1" in
    v[0-9]*.[0-9]*.[0-9]*)
      case "${1#v}" in
        *[!0-9.]* | *.*.*.*)
          return 1
          ;;
        *)
          return 0
          ;;
      esac
      ;;
    *)
      return 1
      ;;
  esac
}

release_tags_from_json() {
  sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1"
}

latest_mark_release_tag_from_json() {
  for candidate in $(release_tags_from_json "$1"); do
    if is_mark_release_tag "$candidate"; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
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
    echo "mark $action: unsupported OS: $(uname -s)" >&2
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
    echo "mark $action: unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

target="$arch-$platform"
refuse_managed_install_dir

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
      echo "mark $action: no latest GitHub release exists for $repo" >&2
      echo "mark $action: create a GitHub release with mark-$target assets, or set MARK_VERSION to an existing release" >&2
    fi
    exit 1
  fi

  tag="$(latest_mark_release_tag_from_json "$latest_json" || true)"
  if [ -z "$tag" ]; then
    releases_json="$tmp_dir/releases.json"
    releases_url="https://api.github.com/repos/$repo/releases?per_page=100"
    if ! curl_download "$releases_url" "$releases_json"; then
      print_download_error "could not list releases for $repo" "$releases_url"
      exit 1
    fi

    tag="$(latest_mark_release_tag_from_json "$releases_json" || true)"
  fi

  if [ -z "$tag" ]; then
    latest_tag="$(release_tags_from_json "$latest_json" | head -n 1)"
    if [ -n "$latest_tag" ]; then
      echo "mark $action: latest GitHub release for $repo is $latest_tag, which is not a mark binary release" >&2
    else
      echo "mark $action: could not resolve latest release for $repo: response did not contain tag_name" >&2
    fi
    echo "mark $action: expected a release tagged like v0.2.0 with mark-$target assets" >&2
    exit 1
  fi
else
  case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
  esac
fi

package="mark-$tag-$target"
asset="$package.tar.gz"
base_url="https://github.com/$repo/releases/download/$tag"

print_plan

cd "$tmp_dir"
if ! curl_download "$base_url/$asset" "$asset"; then
  print_download_error "could not download release asset $asset" "$base_url/$asset"
  echo "mark $action: release $tag for $repo must include $asset" >&2
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
    echo "mark $action: warning: shasum or sha256sum not found; skipping checksum verification" >&2
  else
    echo "mark $action: shasum or sha256sum not found; set MARK_ALLOW_UNVERIFIED=1 to skip checksum verification" >&2
    exit 1
  fi
elif allow_unverified; then
  echo "mark $action: warning: checksum file not available; skipping checksum verification" >&2
else
  print_download_error "could not download checksum $checksum" "$base_url/$checksum"
  echo "mark $action: checksum file not available; set MARK_ALLOW_UNVERIFIED=1 to skip checksum verification" >&2
  exit 1
fi

tar -xzf "$asset"
install_source="$package/mark"
if [ ! -d "$package" ] || [ ! -x "$install_source" ]; then
  echo "mark $action: extracted archive does not contain executable $install_source" >&2
  exit 1
fi

mkdir -p "$install_dir"
install -m 755 "$install_source" "$install_dir/$binary"

if [ "$action" = "update" ]; then
  print_success "Updated $binary to $tag"
else
  print_success "Installed $binary $tag"
  printf 'Run: %s\n' "$binary"
  printf 'Config: %s config\n' "$binary"
  print_path_hint
fi
