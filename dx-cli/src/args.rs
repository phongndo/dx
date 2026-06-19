use std::path::PathBuf;

use clap::{
    Args, Parser, Subcommand,
    builder::styling::{AnsiColor, Styles},
};

pub(crate) const HELP_TEMPLATE: &str = "\
{about-with-newline}
usage:
  {usage}

commands:
{subcommands}

options:
{options}

examples:
  dx
  dx diff --staged
  dx diff --unstaged
  dx diff --base main
  dx diff main feature
  dx show
  dx show HEAD~1
  dx show review 123
  dx show review https://github.com/owner/repo/pull/123
  dx patch changes.diff
  cat changes.diff | dx patch -
  dx diff --no-watch
  dx diff --no-syntax
  dx diff --stat
  dx config
  dx syntax add ruby elixir";

pub(crate) const INSTALL_SCRIPT: &str = include_str!("../../scripts/install.sh");
pub(crate) const RELEASE_REPO: &str = "phongndo/dx";

#[derive(Debug, Parser)]
#[command(
    name = "dx",
    version,
    about = "Terminal Git diff review tool",
    help_template = HELP_TEMPLATE,
    next_help_heading = "options",
    subcommand_help_heading = "commands",
    styles = help_styles()
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

pub(crate) fn help_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Cyan.on_default().bold())
        .usage(AnsiColor::Cyan.on_default().bold())
        .literal(AnsiColor::White.on_default().bold())
        .placeholder(AnsiColor::White.on_default())
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    #[command(
        about = "Review a Git diff",
        after_help = "\
examples:
  dx diff
  dx diff --base main
  dx diff --staged
  dx diff main feature"
    )]
    Diff(DiffArgs),
    #[command(
        about = "Review a Git revision or hosted review",
        after_help = "\
examples:
  dx show
  dx show HEAD~1
  dx show review 123
  dx show review https://github.com/owner/repo/pull/123"
    )]
    Show(ShowArgs),
    #[command(
        about = "Review an existing unified diff",
        after_help = "\
examples:
  dx patch changes.diff
  cat changes.diff | dx patch -"
    )]
    Patch(PatchArgs),
    #[command(
        alias = "ts",
        alias = "tree-sitter",
        about = "Manage syntax highlighting languages"
    )]
    Syntax {
        #[command(subcommand)]
        command: SyntaxCommand,
    },
    #[command(about = "Print the user config file path")]
    Config,
    #[command(
        about = "Update this curl-installed dx binary from GitHub releases",
        after_help = "\
examples:
  dx update
  dx update --target-version 0.1.1
  dx update --install-dir ~/.local/bin"
    )]
    Update(UpdateArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum SyntaxCommand {
    #[command(about = "Install and enable syntax highlighting languages")]
    Add(SyntaxAddArgs),
    #[command(about = "Update cached syntax highlighting parsers")]
    Update(SyntaxUpdateArgs),
    #[command(alias = "remove", about = "Remove syntax highlighting languages")]
    Rm(SyntaxLanguagesArgs),
    #[command(
        visible_alias = "ls",
        about = "List installed and enabled syntax highlighting languages"
    )]
    List,
    #[command(about = "List syntax highlighting languages")]
    Available(SyntaxAvailableArgs),
    #[command(about = "Remove cached tree-sitter parser libraries")]
    Clean,
    #[command(about = "Print tree-sitter cache and user config paths")]
    Path,
    #[command(about = "Validate enabled syntax highlighting languages")]
    Doctor,
}

#[derive(Debug, Args)]
pub(crate) struct SyntaxAddArgs {
    #[arg(value_name = "LANG", required = true)]
    pub(crate) languages: Vec<String>,
    /// Register a local tree-sitter parser library for this language.
    #[arg(long, value_name = "PATH")]
    pub(crate) parser: Option<PathBuf>,
    /// Install a local tree-sitter highlights.scm query for this language.
    #[arg(long, value_name = "PATH")]
    pub(crate) query: Option<PathBuf>,
    /// Map a file extension to this language. Can be repeated.
    #[arg(long = "ext", value_name = "EXT")]
    pub(crate) extensions: Vec<String>,
    /// Map an exact filename to this language. Can be repeated.
    #[arg(long = "filename", value_name = "NAME")]
    pub(crate) filenames: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct SyntaxLanguagesArgs {
    #[arg(value_name = "LANG", required = true)]
    pub(crate) languages: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct SyntaxUpdateArgs {
    #[arg(value_name = "LANG", required_unless_present = "all")]
    pub(crate) languages: Vec<String>,
    #[arg(long, conflicts_with = "languages")]
    pub(crate) all: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SyntaxAvailableArgs {
    #[arg(long, conflicts_with = "enabled")]
    pub(crate) installed: bool,
    #[arg(long, conflicts_with = "installed")]
    pub(crate) enabled: bool,
}

#[derive(Debug, Args, Default)]
pub(crate) struct DiffArgs {
    #[arg(value_name = "REV", num_args = 0..=2)]
    pub(crate) revs: Vec<String>,
    #[arg(short = 'r', long)]
    pub(crate) repo: Option<PathBuf>,
    #[arg(short = 'b', long)]
    pub(crate) base: Option<String>,
    #[arg(long, conflicts_with = "unstaged", conflicts_with_all = ["base", "revs"])]
    pub(crate) staged: bool,
    #[arg(long, conflicts_with_all = ["base", "revs"])]
    pub(crate) unstaged: bool,
    #[arg(long = "no-untracked")]
    pub(crate) no_untracked: bool,
    /// Disable live reload in the interactive diff viewer.
    #[arg(long = "no-watch")]
    pub(crate) no_watch: bool,
    /// Disable syntax highlighting in the interactive diff viewer.
    #[arg(long = "no-syntax")]
    pub(crate) no_syntax: bool,
    #[arg(short = 's', long)]
    pub(crate) stat: bool,
}

#[derive(Debug, Args, Default)]
pub(crate) struct ShowArgs {
    /// Revision to show, or `review TARGET` for a hosted review.
    #[arg(value_name = "TARGET", num_args = 0..=2)]
    pub(crate) targets: Vec<String>,
    #[arg(short = 'r', long)]
    pub(crate) repo: Option<PathBuf>,
    /// Disable syntax highlighting in the interactive diff viewer.
    #[arg(long = "no-syntax")]
    pub(crate) no_syntax: bool,
    #[arg(short = 's', long)]
    pub(crate) stat: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PatchArgs {
    /// Unified diff file to review, or stdin when FILE is `-`.
    #[arg(value_name = "FILE")]
    pub(crate) path: PathBuf,
    #[arg(short = 'r', long)]
    pub(crate) repo: Option<PathBuf>,
    /// Disable syntax highlighting in the interactive diff viewer.
    #[arg(long = "no-syntax")]
    pub(crate) no_syntax: bool,
    #[arg(short = 's', long)]
    pub(crate) stat: bool,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateArgs {
    /// Release version to install, without or with the leading v.
    #[arg(long = "target-version", value_name = "VERSION")]
    pub(crate) version: Option<String>,
    /// Directory to update. Defaults to the directory containing the invoked dx.
    #[arg(long, value_name = "DIR")]
    pub(crate) install_dir: Option<PathBuf>,
}
