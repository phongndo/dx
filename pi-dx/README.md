# pi-dx

Pi extension that adds `/diff` and opens the external `dx` terminal diff
reviewer from inside Pi.

`dx` is not bundled with this package. Install `dx` separately and keep it on
`PATH`, or set `PI_DX_BIN` to the executable path.

## Install

Install `dx` first:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | sh
```

Then install the Pi package from npm:

```sh
pi install npm:pi-dx
```

Use a non-`PATH` binary with:

```sh
PI_DX_BIN=/path/to/dx pi
```

## Development

Run the extension from this checkout without installing the npm package:

```sh
pi -e ./extensions/pi-dx.ts
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

The external `dx` terminal UI opens immediately from interactive Pi, including
while an agent turn is still running. Pi's TUI is restored when `dx` exits.

`/diff --patch -` is intentionally rejected because Pi cannot pipe stdin into
the external viewer from a slash command. Write the patch to a file and pass the
file path instead.

## Current error behavior

- Missing `dx`: shows an install hint.
- Non-interactive Pi mode: refuses to run because `dx` needs a terminal.
- No Git repo for Git-backed diffs: shows a clean error. Future agent turn diff
  support can use this branch as the fallback path.
- Malformed `/diff` quoting or non-zero `dx` exit: shows a Pi notification.
