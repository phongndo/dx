# dx

`dx` is a performant keyboard-first terminal Git diff reviewer.

## What dx does

- Opens the current worktree diff in an interactive terminal UI.
- Streams unified diff output when stdout is not a terminal, or with `--stat`.
- Reviews staged, unstaged, branch, revision-range, patch-file, stdin, and GitHub PR diffs.
- Watches local worktree-backed diffs and reloads the view as files change.
- Supports syntax highlighting with bundled Tree-sitter languages and optional parser caches.

## Install

Install the latest release with the shell installer:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | sh
```

The curl installer is the only supported install path for now. Homebrew, mise,
Cargo, and other package-manager installs are deprecated; reinstall with the
command above if you used one of those paths before.

Installer environment variables use the `DX_` prefix:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | DX_VERSION=0.2.0 sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | DX_INSTALL_DIR=/usr/local/bin sh
```

Update a curl-installed binary in place:

```sh
dx update
dx update --target-version 0.2.0
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

## Pi extension

This repository includes a separate `pi-dx` Pi package that adds a `/diff`
command to Pi and shells out to an already-installed `dx` binary. It does not
bundle the CLI. Install the published package from npm:

```sh
pi install npm:pi-dx
```

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

Custom Tree-sitter support can be registered without rebuilding `dx`:

```sh
dx syntax add mylang \
  --parser ~/parsers/libtree_sitter_mylang.dylib \
  --query ~/parsers/mylang/highlights.scm \
  --ext mylang
```

User highlight queries are read from `~/.config/dx/queries/<lang>/highlights.scm`
and take precedence over bundled queries.

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
