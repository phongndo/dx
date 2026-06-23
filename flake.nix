{
  description = "mark development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];

      forAllSystems =
        function:
        nixpkgs.lib.genAttrs systems (
          system:
          function {
            pkgs = import nixpkgs { inherit system; };
          }
        );
    in
    {
      devShells = forAllSystems (
        { pkgs }:
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              coreutils
              curl
              git
              gnutar
              just
              nodejs_24
              pnpm_11
              rust-analyzer
              rustc
              rustfmt
              zsh
            ];
            shellHook = ''
              mark_dev_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
              mark_dev_bin="$mark_dev_root/target/dev-bin"
              mark_dev_zdotdir="$mark_dev_root/target/dev-zdotdir"

              export MARK_DEV_ROOT="$mark_dev_root"
              export MARK_DEV_BIN="$mark_dev_bin"
              export MARK_DEV_ZDOTDIR="$mark_dev_zdotdir"

              mkdir -p "$MARK_DEV_BIN" "$MARK_DEV_ZDOTDIR"
              cat > "$MARK_DEV_BIN/mark" <<'MARK_DEV_SHIM'
#!/usr/bin/env sh
set -eu

repo="''${MARK_DEV_ROOT:?MARK_DEV_ROOT is not set}"
binary="$repo/target/debug/mark"

needs_build=0
if [ ! -x "$binary" ]; then
  needs_build=1
else
  newer_source="$(find "$repo" \( -path "$repo/target" -o -path "$repo/.git" \) -prune -o -type f \( -name '*.rs' -o -name Cargo.toml -o -name Cargo.lock \) -newer "$binary" -print | sed -n '1p')"
  if [ -n "$newer_source" ]; then
    needs_build=1
  fi
fi

if [ "$needs_build" -eq 1 ]; then
  echo "mark dev shim: building mark-cli..." >&2
  (cd "$repo" && cargo build -p mark-cli --locked >&2)
fi

exec "$binary" "$@"
MARK_DEV_SHIM
              chmod +x "$MARK_DEV_BIN/mark"
              export PATH="$MARK_DEV_BIN:$PATH"

              cat > "$MARK_DEV_ZDOTDIR/.zshrc" <<'MARK_DEV_ZSHRC'
export PATH="$MARK_DEV_BIN:$PATH"
MARK_DEV_ZSHRC

              if [ -z "''${MARK_DEV_INTERACTIVE_SHELL:-}" ] && [ -t 0 ] && [ -t 1 ] && command -v zsh >/dev/null 2>&1; then
                export MARK_DEV_INTERACTIVE_SHELL=1
                exec env ZDOTDIR="$MARK_DEV_ZDOTDIR" zsh -i
              fi
            '';
          };
        }
      );
    };
}
