# Configuration

`dx` works without a config file. Create one only when you want to override
syntax behavior, colors, diff rendering, highlight performance limits, or
keybindings.

Print the exact config path for the current machine:

```sh
dx config
```

On XDG systems this is usually `~/.config/dx/config.toml`. `XDG_CONFIG_HOME` is
honored. Windows uses `APPDATA` when `XDG_CONFIG_HOME` is unset.

Parser registry state is stored separately as `tree-sitter.json` under the same
`dx` config directory. Parser cache files live under the user cache directory.
Inspect all syntax paths with:

```sh
dx syntax path
```

## Example

```toml
mode = "enabled"
colorscheme = "system"
transparent_background = false

[diff]
line_background = "subtle"
gutter_background = "delta"
inline_background = "strong"
sign_style = "bold"
context_expand = 20

[limits]
max_source_kib = 1024
max_line_kib = 8
cache_entries = 512
queue_entries = 512
prefetch_viewports = 1

[keymap.global]
leader = "space"
help = "?"
reload = "r"
file_filter = "f"
grep = "/"
diff_menu = "space m"
options_menu = "space o"
file_browser = "space b"
quit = "q"
layout = "space s"
edit_hunk = "ctrl-g"
copy_error_log = "ctrl-shift-c"
next_diff_type = "tab"
previous_diff_type = "shift-tab"

[keymap.menu]
up = ["k", "up", "shift-tab"]
down = ["j", "down", "tab"]
select = "space"
confirm = "enter"
close = ["esc", "q"]
```

## Syntax mode

`mode` controls which languages are eligible for syntax highlighting:

```toml
mode = "enabled"
```

Supported values:

- `enabled` - core languages plus languages enabled with `dx syntax add`.
- `builtin` - all bundled languages with parser and highlight support.
- `all` - bundled languages plus trusted installed parser caches.

Use `dx --no-syntax`, `dx diff --no-syntax`, `dx show --no-syntax`, or
`dx patch --no-syntax changes.diff` to disable syntax highlighting for one run.

## Colorschemes and colors

Use a built-in colorscheme by name:

```toml
colorscheme = "system"
```

The aliases `ansi` and `terminal` use terminal colors. Base16 themes can be
loaded from a file:

```toml
[colorscheme]
source = "base16"
path = "~/themes/example.yaml"
```

Individual colors can be overridden at the top level or in `[colors]`:

```toml
bg = "#111315"
fg = "#d8dee9"

[colors]
addition_bg = "#1f3025"
deletion_bg = "#372526"
keyword = "ansi-5"
string = "green"
```

Color values support hex colors, ANSI indexes such as `ansi-5`, and named
terminal colors.

Common override keys include:

```text
bg, fg, header, file, hunk, notice, cursor, muted, gutter_bg, empty_diff,
search_match_fg, search_match_bg,
addition_fg, addition_gutter_bg, addition_bg, addition_inline_bg,
deletion_fg, deletion_gutter_bg, deletion_bg, deletion_inline_bg,
attribute, comment, constant, constructor, function, keyword, label, module,
number, operator, property, punctuation, string, tag, type, variable
```

## Diff rendering

`[diff]` controls visual emphasis:

```toml
[diff]
line_background = "subtle"   # none, subtle, strong
gutter_background = "delta"  # base, delta
inline_background = "strong" # none, subtle, strong
sign_style = "bold"          # normal, bold
context_expand = 20          # number of lines, or "full"
```

`word_background` and `word_diff_background` are accepted aliases for
`inline_background`. `context_lines`, `context_expand`, and `expand_context` are
accepted aliases for `context_expand`.

## Highlight limits

Large files and very long lines are intentionally bounded:

```toml
[limits]
max_source_kib = 1024
max_line_kib = 8
cache_entries = 512
queue_entries = 512
prefetch_viewports = 1
```

## Keybindings

Global multi-key bindings must start with the configured leader key. Menu
bindings are single-key and apply to the diff source and options menus.

Bindings can be a string or a list of strings:

```toml
[keymap.global]
leader = ","
help = ["?", ", h"]
file_filter = ", f"
grep = ", /"
quit = "q"

[keymap.menu]
up = ["k", "up"]
down = ["j", "down"]
select = "space"
confirm = "enter"
close = ["esc", "q"]
```

Key names include printable characters plus names such as `space`, `enter`,
`esc`, `tab`, `shift-tab`, `up`, `down`, `left`, `right`, and modified keys such
as `ctrl-g`.

If the keymap cannot be parsed, `dx` ignores it for that run and shows a notice
inside the TUI instead of failing the diff review.
