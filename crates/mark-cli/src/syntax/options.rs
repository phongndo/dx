use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use mark_core::{MarkError, MarkResult};

use crate::args::{DiffArgs, DifftoolArgs, PatchArgs, ReviewArgs, ShowArgs};

fn diff_output(stat: bool) -> mark_command::DiffOutput {
    if stat {
        mark_command::DiffOutput::Stat
    } else {
        mark_command::DiffOutput::Patch
    }
}

pub(crate) fn diff_options(args: DiffArgs) -> MarkResult<mark_command::DiffOptions> {
    let scope = if args.staged {
        mark_command::DiffScope::Staged
    } else if args.unstaged {
        mark_command::DiffScope::Unstaged
    } else {
        mark_command::DiffScope::All
    };

    let source = match (args.base, args.revs.as_slice()) {
        (Some(base), []) => mark_command::DiffSource::Base(base.into()),
        (Some(_), _) => {
            return Err(MarkError::Usage(
                "use either --base or positional revisions, not both".to_owned(),
            ));
        }
        (None, []) => mark_command::DiffSource::Worktree { scope },
        (None, [base]) => mark_command::DiffSource::Base(base.clone().into()),
        (None, [left, right]) => mark_command::DiffSource::Range {
            left: left.clone().into(),
            right: right.clone().into(),
        },
        (None, _) => {
            return Err(MarkError::Usage(
                "mark accepts at most two revisions".to_owned(),
            ));
        }
    };

    if scope != mark_command::DiffScope::All
        && !matches!(source, mark_command::DiffSource::Worktree { .. })
    {
        return Err(MarkError::Usage(
            "--staged and --unstaged only apply to working tree diffs".to_owned(),
        ));
    }

    Ok(mark_command::DiffOptions {
        repo: args.repo.repo.map(Into::into),
        source,
        local_untracked: mark_command::UntrackedMode::from_include(!args.no_untracked),
        output: diff_output(args.display.stat),
    })
}

pub(crate) fn show_options(args: ShowArgs) -> MarkResult<mark_command::DiffOptions> {
    Ok(mark_command::DiffOptions {
        repo: args.repo.repo.map(Into::into),
        source: mark_command::DiffSource::Show(
            args.rev.unwrap_or_else(|| "HEAD".to_owned()).into(),
        ),
        local_untracked: mark_command::UntrackedMode::Exclude,
        output: diff_output(args.display.stat),
    })
}

pub(crate) fn review_options(args: ReviewArgs) -> MarkResult<mark_command::DiffOptions> {
    mark_command::review_diff_options(args.repo.repo, &args.target, args.display.stat)
}

pub(crate) fn difftool_options(args: DifftoolArgs) -> MarkResult<mark_command::DiffOptions> {
    Ok(mark_command::DiffOptions {
        repo: args.repo.repo.map(Into::into),
        source: mark_command::DiffSource::Difftool {
            left: args.left.into(),
            right: args.right.into(),
            path: args.path.map(Into::into),
        },
        local_untracked: mark_command::UntrackedMode::Exclude,
        output: diff_output(args.display.stat),
    })
}

pub(crate) fn patch_options(args: PatchArgs) -> MarkResult<mark_command::DiffOptions> {
    Ok(mark_command::DiffOptions {
        repo: args.repo.repo.map(Into::into),
        source: patch_source(args.path)?,
        local_untracked: mark_command::UntrackedMode::Exclude,
        output: diff_output(args.display.stat),
    })
}

pub(crate) fn patch_source(path: PathBuf) -> MarkResult<mark_command::DiffSource> {
    if path == Path::new("-") {
        let mut patch = Vec::new();
        io::stdin().read_to_end(&mut patch)?;
        return Ok(mark_command::DiffSource::Patch(
            mark_command::PatchSource::Stdin(Arc::from(patch.into_boxed_slice())),
        ));
    }

    Ok(mark_command::DiffSource::Patch(
        mark_command::PatchSource::File(path),
    ))
}
