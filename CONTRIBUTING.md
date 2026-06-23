# Contributing

Thanks for improving `mark`. This repo is optimized for careful, maintainable
changes over clever rewrites.

## Standard

- Correct > compliant.
- Simple > clever.
- Inspect, edit, verify.
- Existing style wins.
- Fix root causes, not symptoms.
- Preserve public APIs unless the change is intentional.
- Keep changes local; avoid unrelated cleanup.
- Do not weaken validation, sandboxing, or error handling.

## Before changing code

1. Check the working tree with `git status`.
2. Read the relevant code, tests, configs, and docs.
3. Identify the smallest safe change.
4. Decide the cheapest useful verification command.

Development setup lives in [docs/development.md](docs/development.md).

## Pull requests

Use the PR template and include:

- What changed.
- Why it changed.
- User-visible behavior or compatibility impact.
- Verification commands that were actually run.
- Known risks or follow-up work.

Do not mark a check as passed unless you ran it.

## Documentation

Update docs with user-visible changes to:

- CLI commands, flags, aliases, and examples.
- Config keys and accepted aliases.
- Installer environment variables.
- Release process or asset naming.
- `pi-mark` slash command and package behavior.

Keep the README as the product entry point. Put detailed command, config, and
development material in `docs/` or `pi-mark/README.md`.
