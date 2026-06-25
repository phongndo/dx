# pi-mark

Pi extension that adds a `/mark` command and opens the external `mark` terminal
diff reviewer from inside Pi.

`mark` is not bundled with this package. Install `mark` separately and keep it on
`PATH`, or set `PI_MARK_BIN` to the executable path.

## Install

Install `mark` first:

```sh
curl -fsSL https://raw.githubusercontent.com/phongndo/mark/main/scripts/install.sh | sh
```

Then install the Pi package from npm:

```sh
pi install npm:pi-mark
```

## Migrating from pi-dx

`pi-dx` was the old package name for the `dx` binary. Remove it and install
`pi-mark`:

```sh
pi remove npm:pi-dx
pi install npm:pi-mark
```

If `pi-dx` is listed in project settings, replace `npm:pi-dx` with
`npm:pi-mark` in `.pi/settings.json` and run `pi update --extensions`.

Command and environment changes:

- `/diff` -> `/mark` or `/mark diff`
- `/show` -> `/mark show`
- `/patch` -> `/mark patch`
- `PI_DX_BIN` -> `PI_MARK_BIN`

Use a non-`PATH` binary with:

```sh
PI_MARK_BIN=/path/to/mark pi
```

## Development

Run the extension from this repository root without installing the npm package:

```sh
pi -e ./pi-mark/extensions/pi-mark.ts
```

Developer checks use pnpm:

```sh
cd pi-mark
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

Publishing is manual, matching the main `mark` binary release flow:

1. Update `pi-mark/package.json` version.
2. Merge the change.
3. Run the `Publish pi-mark` GitHub Actions workflow.

The workflow validates the package, publishes it to npm with provenance, and can
create a `pi-mark-vX.Y.Z` GitHub release. npm trusted publishing must be configured
for this repository and workflow before the publish step can succeed.

## Usage

```text
/mark
/mark diff --staged
/mark diff --unstaged
/mark diff --base main
/mark diff main feature
/mark show
/mark show HEAD~1
/mark review 123
/mark review https://github.com/owner/repo/pull/123
/mark patch changes.diff
```

Hosted reviews currently resolve GitHub pull requests.

The external `mark` terminal UI opens immediately from interactive Pi, including
while an agent turn is still running. Pi's TUI is restored when `mark` exits.

`/mark patch -` is intentionally rejected because Pi cannot pipe stdin into the
external viewer from a slash command. Write the patch to a file and pass the
file path instead.

## Current error behavior

- Missing `mark`: shows an install hint.
- Non-interactive Pi mode: refuses to run because `mark` needs a terminal.
- No Git repo for Git-backed diffs or revision shows: shows a clean error.
  Future agent turn diff support can use this branch as the fallback path.
- Malformed slash-command quoting or non-zero `mark` exit: shows a Pi
  notification.
