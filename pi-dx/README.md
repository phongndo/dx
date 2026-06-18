# pi-dx

Pi extension that adds `/diff` and opens the external `dx` terminal diff
reviewer from inside Pi.

`dx` is not bundled with this package. Install `dx` separately and keep it on
`PATH`, or set `PI_DX_BIN` to the executable path.

## Install

From this checkout:

```sh
pi install ./pi-dx
```

For development without installing:

```sh
pi -e ./pi-dx/extensions/pi-dx.ts
```

Developer checks use pnpm:

```sh
cd pi-dx
pnpm install
pnpm run check
```

Useful individual commands:

```sh
pnpm run typecheck
pnpm run lint
pnpm run format:check
pnpm run format
```

## Release

Publishing is manual, matching the main `dx` binary release flow:

1. Update `pi-dx/package.json` version.
2. Merge the change.
3. Run the `Publish pi-dx` GitHub Actions workflow.

The workflow validates the package, publishes it to npm with provenance, and can
create a `pi-dx-vX.Y.Z` GitHub release. npm trusted publishing must be configured
for this repository and workflow before the publish step can succeed.

Install `dx` with:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | sh
```

Use a non-`PATH` binary with:

```sh
PI_DX_BIN=/path/to/dx pi
```

## Usage

```text
/diff
/diff --staged
/diff --unstaged
/diff --base main
/diff main feature
/diff --pr 123
/diff --pr https://github.com/owner/repo/pull/123
/diff --patch changes.diff
```

`/diff --patch -` is intentionally rejected because Pi cannot pipe stdin into
the external viewer from a slash command. Write the patch to a file and pass the
file path instead.

## Current error behavior

- Missing `dx`: shows an install hint.
- Non-interactive Pi mode: refuses to run because `dx` needs a terminal.
- No Git repo for Git-backed diffs: shows a clean error. Future agent turn diff
  support can use this branch as the fallback path.
- Malformed `/diff` quoting or non-zero `dx` exit: shows a Pi notification.
