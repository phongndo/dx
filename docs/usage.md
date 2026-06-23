# Usage

`dx` reviews Git diffs in an interactive terminal UI when stdout is a terminal.
When stdout is not a terminal it streams rendered diff output instead. When
`--stat` is requested it streams diff statistics instead of opening the UI.

Run `dx --help` for the authoritative command list.

## Diff sources

`dx` is a shortcut for `dx diff`:

```sh
dx
dx diff
```

Common local review modes:

```sh
dx diff --staged
dx diff --unstaged
dx diff --no-untracked
dx diff --base main
dx diff main feature
```

Use `--repo` when running from outside the target repository:

```sh
dx diff --repo ../project --staged
dx show --repo ../project HEAD~1
```

Use `--no-watch` to disable local worktree reloads for one run, and
`--no-syntax` to disable syntax highlighting for one run:

```sh
dx diff --no-watch
dx diff --no-syntax
```

Use `--stat` to print summary statistics instead of opening the interactive UI:

```sh
dx diff --stat
dx show HEAD~1 --stat
```

## Revisions and hosted reviews

`dx show` reviews a revision. With no target it shows `HEAD`:

```sh
dx show
dx show HEAD~1
```

Hosted reviews currently support GitHub pull requests:

```sh
dx show review 123
dx show review https://github.com/owner/repo/pull/123
dx diff --pr 123
dx diff --pr https://github.com/owner/repo/pull/123
```

Numeric pull request targets are resolved from the current repository's
`origin` remote. Full GitHub pull request URLs do not need a local repository.
Fetching pull requests uses `curl`. Set `GH_TOKEN` or `GITHUB_TOKEN` for
private repositories or higher rate limits.

## Patch files and stdin

Review an existing unified diff:

```sh
dx patch changes.diff
cat changes.diff | dx patch -
```

The older top-level form still works:

```sh
dx --patch changes.diff
```

## Pager mode

Use `dx pager` for `git diff` and `git show` output:

```sh
git config --global core.pager "dx pager"
git diff | dx pager
```

`dx pager` reads stdin. Diff input opens the interactive reviewer when possible
and falls back to static ANSI output in captured pager hosts such as lazygit.
Non-diff input is passed through the user's text pager.

Static diff output reuses dx's renderer, colorscheme, syntax highlighting, and
layout. Override the static layout when needed:

```sh
dx pager --layout split
dx pager --layout unified
dx pager --no-syntax
```

## Difftool mode

Configure Git to launch `dx` for Git-provided file pairs:

```sh
git config --global diff.tool dx
git config --global difftool.dx.cmd 'dx difftool -- "$LOCAL" "$REMOTE" "$MERGED"'
```

Git sets `$LOCAL` to the pre-image, `$REMOTE` to the post-image, and `$MERGED`
to the display path. `dx difftool` turns that pair into a normal review:

```sh
git difftool HEAD -- src/file.rs
dx difftool -- "$LOCAL" "$REMOTE" "$MERGED"
dx difftool --watch -- "$LOCAL" "$REMOTE" "$MERGED"
```

## Interactive controls

Common default controls:

```text
q / Ctrl-C     quit
?              help
j / Down       scroll down or move focus
k / Up         scroll up or move focus
d / PageDown   page down
u / PageUp     page up
g              top
G              bottom
]              next hunk
[              previous hunk
f              file filter
/              grep filter
n / p          next / previous grep match
r              reload
Space m        diff type selector
Space o        settings menu
Space b        file browser
Space s        toggle split/unified layout
Ctrl-G         open the focused hunk in `$VISUAL`, `$GIT_EDITOR`, or `$EDITOR`
Tab            next diff type
Shift-Tab      previous diff type
```

Selector panes keep focus in the filter input: type to filter, Enter selects or
toggles, Esc closes, and arrows, Tab / Shift-Tab, or Ctrl-N / Ctrl-P move the
highlighted row. Settings with multiple values can also cycle with Left / Right.

Keybindings can be customized in the user config file. See
[configuration](configuration.md#keybindings).

## Syntax languages

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

## Pi package

The `pi-dx` package adds `/diff`, `/show`, and `/patch` slash commands to Pi.
It shells out to `dx`, so install the CLI first and keep it on `PATH`, or set
`PI_DX_BIN` to the executable path.

See [`../pi-dx/README.md`](../pi-dx/README.md).
