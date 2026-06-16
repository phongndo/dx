# dx

`dx` is a keyboard-first terminal Git diff reviewer. It was split out of `hz`
so workspace isolation and diff review can evolve as separate products.

## What dx does

- Opens the current worktree diff in an interactive terminal UI.
- Streams unified diff output when stdout is not a terminal, or with `--stat`.
- Reviews staged, unstaged, branch, revision-range, patch-file, stdin, and GitHub PR diffs.
- Watches local worktree-backed diffs and reloads the view as files change.
- Supports syntax highlighting with bundled Tree-sitter languages and optional parser caches.

## Install

```sh
brew install phongndo/tap/dx-cli
```

Or use the shell installer:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | sh
```

Installer environment variables use the `DX_` prefix:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | DX_VERSION=0.1.1 sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | DX_INSTALL_DIR=/usr/local/bin sh
```

Update an installer-managed binary in place:

```sh
dx update
dx update --target-version 0.1.1
```

With Cargo:

```sh
cargo install --locked --git https://github.com/phongndo/dx --tag v0.1.1 dx-cli
```

## Release

Push a `vX.Y.Z` tag, or run the Release workflow manually, to build release
binaries and publish the GitHub release assets used by the shell installer.

## Usage

```sh
dx
dx --staged
dx --unstaged
dx --no-untracked
dx --base main
dx main feature
dx --pr 123
dx --pr https://github.com/owner/repo/pull/123
dx --patch changes.diff
cat changes.diff | dx --patch -
dx --no-watch
dx --no-syntax
dx --stat
dx config
```

`dx diff ...` is also accepted as a compatibility/discoverability alias, but
plain `dx ...` is the primary interface.

## Configuration

`dx` reads a user-local TOML config from the user's config directory. On XDG
systems this is usually `~/.config/dx/config.toml`; run `dx config` to
print the exact path. `XDG_CONFIG_HOME` is honored, and Windows uses `APPDATA`
when `XDG_CONFIG_HOME` is unset.

No config file is created automatically; missing config means built-in defaults.
Create the file manually only when you want to override syntax mode,
colorscheme, diff styling, or highlight performance limits. Parser registry
state is kept separately in `tree-sitter.json` under the same `dx` config
directory.

## Syntax highlighting

Core languages are bundled. Extra languages can be installed and managed with:

```sh
dx syntax add ruby elixir
dx syntax update --all
dx syntax available --installed
dx syntax rm ruby
dx syntax list
dx syntax doctor
dx syntax clean
dx syntax path
```

Syntax settings and caches live under the user config/cache locations for `dx`
(for example `~/.config/dx/config.toml`).

## Workspace layout

```text
dx-cli       command parsing, update, and CLI UX
dx-command   thin command facade for diff and syntax actions
dx-core      shared errors and common path helpers
dx-git       low-level Git integration boundary
dx-diff      diff loading and plain rendering boundary
dx-syntax    Tree-sitter syntax highlighting and parser cache management
dx-tui       ratatui/crossterm diff review UI
dx-bench     local benchmark fixture generation
```
