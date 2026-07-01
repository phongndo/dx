# mark

[![Quality](https://github.com/phongndo/mark/actions/workflows/quality.yml/badge.svg?branch=main)](https://github.com/phongndo/mark/actions/workflows/quality.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`mark` is a fast, keyboard-first terminal Git diff reviewer.

Use it when you want to review real diffs without leaving the terminal: local
worktree changes, revision ranges, patch files, Git difftool pairs, pager input,
and GitHub pull requests.

## Why mark

- Interactive terminal UI for large unified diffs.
- Local worktree watching with explicit reload controls.
- Split and unified diff layouts with syntax highlighting.
- Git pager and Git difftool integrations.
- Patch-file and stdin diff review for generated changes.
- GitHub pull request review by number or URL.
- Optional Pi package with a `/mark` slash command.

## Install

The supported install path is the shell installer for macOS and Linux on
`aarch64` and `x86_64`:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/mark/main/scripts/install.sh | sh
```

Homebrew, mise, Cargo, and other package-manager installs are deprecated for
now. Reinstall with the command above if you used one of those paths before.

Installer environment variables use the `MARK_` prefix:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/mark/main/scripts/install.sh | MARK_VERSION=0.7.2 sh
curl -fsSL https://raw.githubusercontent.com/phongndo/mark/main/scripts/install.sh | MARK_INSTALL_DIR=/usr/local/bin sh
```

Update a curl-installed binary in place:

```sh
mark update
mark update --target-version 0.7.2
```

Nightly builds are published from `main` as a prerelease channel. Switch the
installed `mark` binary to nightly, then back to stable, with:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/mark/main/scripts/install.sh | MARK_VERSION=nightly sh
mark update
```

Once on stable, switch to nightly again with:

```sh
mark update --target-version nightly
```

Nightly binaries report their channel and build commit in `mark --version`.

## Quick start

```sh
mark                         # review current worktree changes
mark diff --base main        # review current branch against main
mark diff main feature       # review a revision range
mark show HEAD~1             # review one commit
mark review 123              # review GitHub PR #123 from the current repo
mark patch changes.diff      # review an existing patch file
git diff | mark pager        # use mark as a diff pager
```

Plain `mark` is a shortcut for `mark diff`.

## Git integrations

Use `mark pager` as a Git pager for `git diff` and `git show` output:

```sh
git config --global core.pager "mark pager"
```

Use `mark difftool` as a Git difftool for Git-provided file pairs:

```sh
git config --global diff.tool mark
git config --global difftool.mark.cmd 'mark difftool -- "$LOCAL" "$REMOTE" "$MERGED"'
```

## Pi extension

This repository includes a separate `pi-mark` Pi package. It adds a `/mark`
command to Pi and shells out to an already-installed `mark` binary. It does not
bundle the CLI.

```sh
pi install npm:pi-mark
```

If you previously installed the old `pi-dx` package, migrate with:

```sh
pi remove npm:pi-dx
pi install npm:pi-mark
```

The slash command moved from `/diff`, `/show`, and `/patch` to `/mark` with
subcommands (`/mark diff`, `/mark show`, `/mark patch`). `PI_DX_BIN` is now
`PI_MARK_BIN`.

See [`pi-mark/README.md`](pi-mark/README.md) for package usage and development.

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
cargo build -p mark-cli --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

## Workspace layout

```text
crates/mark-cli       command parsing, update, and CLI UX
crates/mark-command   command facade shared by CLI and future integrations
crates/mark-core      shared errors and path helpers
crates/mark-git       low-level Git process boundary
crates/mark-diff      diff loading, parsing, and plain rendering
crates/mark-syntax    Tree-sitter highlighting and parser cache management
crates/mark-tui       ratatui/crossterm diff review UI
crates/mark-bench     local benchmark fixture generation
pi-mark               Pi extension package published to npm
```

## License

MIT. See [LICENSE](LICENSE).
