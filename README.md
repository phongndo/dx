# dx

[![Quality](https://github.com/phongndo/dx/actions/workflows/quality.yml/badge.svg?branch=main)](https://github.com/phongndo/dx/actions/workflows/quality.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`dx` is a fast, keyboard-first terminal Git diff reviewer.

Use it when you want to review real diffs without leaving the terminal: local
worktree changes, staged changes, revision ranges, patch files, Git difftool
pairs, pager input, and GitHub pull requests.

## Why dx

- Interactive terminal UI for large unified diffs.
- Local worktree watching with explicit reload controls.
- Split and unified diff layouts with syntax highlighting.
- Git pager and Git difftool integrations.
- Patch-file and stdin diff review for generated changes.
- GitHub pull request review by number or URL.
- Optional Pi package with `/diff`, `/show`, and `/patch` slash commands.

## Install

The supported install path is the shell installer for macOS and Linux on
`aarch64` and `x86_64`:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | sh
```

Homebrew, mise, Cargo, and other package-manager installs are deprecated for
now. Reinstall with the command above if you used one of those paths before.

Installer environment variables use the `DX_` prefix:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | DX_VERSION=0.5.1 sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | DX_INSTALL_DIR=/usr/local/bin sh
```

Update a curl-installed binary in place:

```sh
dx update
dx update --target-version 0.5.1
```

## Quick start

```sh
dx                         # review current worktree changes
dx diff --staged           # review staged changes
dx diff --base main        # review current branch against main
dx diff main feature       # review a revision range
dx show HEAD~1             # review one commit
dx show review 123         # review GitHub PR #123 from the current repo
dx patch changes.diff      # review an existing patch file
git diff | dx pager        # use dx as a diff pager
```

Plain `dx` is a shortcut for `dx diff`.

## Git integrations

Use `dx pager` as a Git pager for `git diff` and `git show` output:

```sh
git config --global core.pager "dx pager"
```

Use `dx difftool` as a Git difftool for Git-provided file pairs:

```sh
git config --global diff.tool dx
git config --global difftool.dx.cmd 'dx difftool -- "$LOCAL" "$REMOTE" "$MERGED"'
```

## Pi extension

This repository includes a separate `pi-dx` Pi package. It adds `/diff`,
`/show`, and `/patch` commands to Pi and shells out to an already-installed
`dx` binary. It does not bundle the CLI.

```sh
pi install npm:pi-dx
```

See [`pi-dx/README.md`](pi-dx/README.md) for package usage and development.

## Documentation

- [Usage](docs/usage.md) - commands, diff sources, pager, difftool, and GitHub
  reviews.
- [Configuration](docs/configuration.md) - config paths, syntax settings,
  colors, diff rendering, and keybindings.
- [Development](docs/development.md) - setup, checks, release flow, and local
  Pi package work.
- [Contributing](CONTRIBUTING.md) - repository standard and PR expectations.

## Development

Use the Nix shell when available:

```sh
nix develop
just check
```

Without Nix, install the Rust toolchain from `rust-toolchain.toml` and run:

```sh
cargo fetch --locked
cargo build -p dx-cli --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

## Workspace layout

```text
dx-cli       command parsing, update, and CLI UX
dx-command   command facade shared by CLI and future integrations
dx-core      shared errors and path helpers
dx-git       low-level Git process boundary
dx-diff      diff loading, parsing, and plain rendering
dx-syntax    Tree-sitter highlighting and parser cache management
dx-tui       ratatui/crossterm diff review UI
dx-bench     local benchmark fixture generation
pi-dx        Pi extension package published to npm
```

## License

MIT. See [LICENSE](LICENSE).
