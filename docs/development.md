# Development

This repository favors small, verified changes. Read the relevant code first,
make the smallest safe diff, and document user-visible behavior changes.

## Prerequisites

- Git
- Rust toolchain from [`rust-toolchain.toml`](../rust-toolchain.toml)
- `curl`, `tar`, and `install` for installer smoke tests
- `just` for repository recipes
- Node.js 24 and pnpm 11 for `pi-mark`
- Nix, optional but preferred for a complete local shell

Do not install global tools just to work in the repo when `nix develop` or the
checked-in package manager setup is available.

## Setup

Preferred:

```sh
nix develop
just setup
```

Without Nix:

```sh
cargo fetch --locked
cargo build -p mark-cli --locked
```

Install the optional Git hook:

```sh
git config core.hooksPath .githooks
```

The hook runs `just check` before commits.

## Common commands

```sh
just check
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo build -p mark-cli --locked
```

For the Pi package:

```sh
cd pi-mark
pnpm install
pnpm run check
```

Run the local extension from the repository root with:

```sh
pi -e ./pi-mark/extensions/pi-mark.ts
```

## Verification ladder

Use the cheapest check that proves the change first:

1. `rust-analyzer diagnostics .`
2. `cargo fmt --all --check`
3. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
4. Focused unit test, for example `cargo test -p mark-tui filter`
5. Focused integration or smoke test
6. `cargo test --workspace --all-targets --all-features --locked`
7. `cargo build --workspace --all-targets --all-features --locked`

Full builds are most useful for public API changes, build config changes,
dependency changes, generated code, toolchains, release packaging, or broad
cross-crate behavior.

## Local smoke tests

Installer and update smoke test:

```sh
scripts/smoke-installer-update
```

Interactive error-pane smoke test:

```sh
scripts/test-diff-error-pane
```

The interactive smoke test must run in a terminal.

## Release flow

The main `mark` binary release uses GitHub Releases.

1. Update the workspace package version in [`Cargo.toml`](../Cargo.toml).
2. Merge the change.
3. Push a `vX.Y.Z` tag, or run the Release workflow manually.

The Release workflow builds macOS and Linux assets named like:

```text
mark-vX.Y.Z-aarch64-apple-darwin.tar.gz
mark-vX.Y.Z-x86_64-apple-darwin.tar.gz
mark-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz
mark-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
```

Those names are part of the installer contract.

## Nightly flow

The Nightly workflow publishes the latest `main` commit to a mutable `vnightly`
GitHub prerelease. The installer treats it as an explicit version channel:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/mark/main/scripts/install.sh | MARK_VERSION=nightly sh
```

Nightly replaces the active `mark` binary. Users switch back to the latest
stable semver release with:

```sh
mark update
```

Keep `vnightly` marked as a prerelease and not latest. The installer only
resolves semver tags like `v0.7.1` for the default `latest` channel, so stable
updates do not accidentally install nightly.

Nightly builds set `MARK_BUILD_CHANNEL=nightly`, so `mark --version` includes
the channel and source commit.

## pi-mark release flow

`pi-mark` is published separately to npm.

1. Update `pi-mark/package.json` version.
2. Merge the change.
3. Run the `Publish pi-mark` workflow.

The workflow validates the package, publishes with npm provenance, and can
create a `pi-mark-vX.Y.Z` GitHub release.
