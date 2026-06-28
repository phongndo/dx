# Usage

`mark` reviews Git diffs in an interactive terminal UI when stdout is a terminal.
When stdout is not a terminal it streams rendered diff output instead. When
`--stat` is requested it streams diff statistics instead of opening the UI.

Run `mark --help` for the authoritative command list.

## Diff sources

`mark` is a shortcut for `mark diff`:

```sh
mark
mark diff
```

Common local review modes:

```sh
mark diff --staged
mark diff --unstaged
mark diff --no-untracked
mark diff --base main
mark diff main feature
```

Use `--repo` when running from outside the target repository:

```sh
mark diff --repo ../project --staged
mark show --repo ../project HEAD~1
```

Use `--no-watch` to disable local worktree reloads for one run, and
`--no-syntax` to disable syntax highlighting for one run:

```sh
mark diff --no-watch
mark diff --no-syntax
```

Use `--stat` to print summary statistics instead of opening the interactive UI:

```sh
mark diff --stat
mark show HEAD~1 --stat
```

## Revisions and hosted reviews

`mark show` reviews a revision. With no target it shows `HEAD`:

```sh
mark show
mark show HEAD~1
```

Hosted reviews currently support GitHub pull requests:

```sh
mark review 123
mark review https://github.com/owner/repo/pull/123
```

Numeric pull request targets are resolved from the current repository's
`origin` remote. Full GitHub pull request URLs do not need a local repository.
Fetching pull requests uses `curl`. Set `GH_TOKEN` or `GITHUB_TOKEN` for
private repositories or higher rate limits.

## Patch files and stdin

Review an existing unified diff:

```sh
mark patch changes.diff
cat changes.diff | mark patch -
```

## Pager mode

Use `mark pager` for `git diff` and `git show` output:

```sh
git config --global core.pager "mark pager"
git diff | mark pager
```

`mark pager` reads stdin. Diff input opens the interactive reviewer when possible
and falls back to static ANSI output in captured pager hosts such as lazygit.
Non-diff input is passed through the user's text pager.

Static diff output reuses mark's renderer, colorscheme, syntax highlighting, and
layout. Override the static layout when needed:

```sh
mark pager --layout split
mark pager --layout unified
mark pager --no-syntax
```

## Difftool mode

Configure Git to launch `mark` for Git-provided file pairs:

```sh
git config --global diff.tool mark
git config --global difftool.mark.cmd 'mark difftool -- "$LOCAL" "$REMOTE" "$MERGED"'
```

Git sets `$LOCAL` to the pre-image, `$REMOTE` to the post-image, and `$MERGED`
to the display path. `mark difftool` turns that pair into a normal review:

```sh
git difftool HEAD -- src/file.rs
mark difftool -- "$LOCAL" "$REMOTE" "$MERGED"
mark difftool --watch -- "$LOCAL" "$REMOTE" "$MERGED"
```

## Interactive controls

Common default controls:

```text
q / Ctrl-C     quit
?              help
j / Down       scroll down or move focus
k / Up         scroll up or move focus
d / Ctrl-D / PgDn page down
u / PageUp     page up
g              top
G              bottom
]              next hunk
[              previous hunk
( / )          previous / next file
, / .          expand context up / down
c              collapse expanded context
f              file filter
/              grep filter
n / p          next / previous grep match
r              reload
m m            diff type selector
m h            head branch picker
m b            base branch picker
m c            commit picker
o              settings menu
a              annotation search menu
b              file browser
s              toggle split/unified layout
Ctrl-G         open the focused hunk in `$VISUAL`, `$GIT_EDITOR`, or `$EDITOR`
y              copy marks to the terminal clipboard
Ctrl-U         clear filters
{ / }          previous / next annotation
Ctrl-Shift-C   copy the error log pane to the terminal clipboard
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
mark syntax add ruby elixir
mark syntax update --all
mark syntax available --installed
mark syntax rm ruby
mark syntax list
mark syntax doctor
mark syntax clean
mark syntax path
```

Custom Tree-sitter support can be registered without rebuilding `mark`:

```sh
mark syntax add mylang \
  --parser ~/parsers/libtree_sitter_mylang.dylib \
  --query ~/parsers/mylang/highlights.scm \
  --ext mylang
```

User highlight queries are read from `~/.config/mark/queries/<lang>/highlights.scm`
and take precedence over bundled queries.

## Pi package

The `pi-mark` package adds a `/mark` slash command to Pi. It shells out to
`mark`, so install the CLI first and keep it on `PATH`, or set `PI_MARK_BIN` to
the executable path.

See [`../pi-mark/README.md`](../pi-mark/README.md).
