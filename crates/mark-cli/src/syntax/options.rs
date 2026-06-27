use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use mark_core::{MarkError, MarkResult};

use crate::args::{DiffArgs, DifftoolArgs, PatchArgs, ReviewArgs, ShowArgs};

pub(crate) fn diff_options(args: DiffArgs) -> MarkResult<mark_command::DiffOptions> {
    let source = match (args.base, args.revs.as_slice()) {
        (Some(base), []) => mark_command::DiffSource::Base(base),
        (Some(_), _) => {
            return Err(MarkError::Usage(
                "use either --base or positional revisions, not both".to_owned(),
            ));
        }
        (None, []) => mark_command::DiffSource::Worktree,
        (None, [base]) => mark_command::DiffSource::Base(base.clone()),
        (None, [left, right]) => mark_command::DiffSource::Range {
            left: left.clone(),
            right: right.clone(),
        },
        (None, _) => {
            return Err(MarkError::Usage(
                "mark accepts at most two revisions".to_owned(),
            ));
        }
    };

    let scope = if args.staged {
        mark_command::DiffScope::Staged
    } else if args.unstaged {
        mark_command::DiffScope::Unstaged
    } else {
        mark_command::DiffScope::All
    };

    Ok(mark_command::DiffOptions {
        repo: args.repo.repo,
        source,
        scope,
        include_untracked: !args.no_untracked,
        stat: args.display.stat,
    })
}

pub(crate) fn show_options(args: ShowArgs) -> MarkResult<mark_command::DiffOptions> {
    Ok(mark_command::DiffOptions {
        repo: args.repo.repo,
        source: mark_command::DiffSource::Show(args.rev.unwrap_or_else(|| "HEAD".to_owned())),
        scope: mark_command::DiffScope::All,
        include_untracked: false,
        stat: args.display.stat,
    })
}

pub(crate) fn review_options(args: ReviewArgs) -> MarkResult<mark_command::DiffOptions> {
    mark_command::review_diff_options(args.repo.repo, &args.target, args.display.stat)
}

pub(crate) fn difftool_options(args: DifftoolArgs) -> MarkResult<mark_command::DiffOptions> {
    Ok(mark_command::DiffOptions {
        repo: args.repo.repo,
        source: mark_command::DiffSource::Difftool {
            left: args.left,
            right: args.right,
            path: args.path,
        },
        scope: mark_command::DiffScope::All,
        include_untracked: false,
        stat: args.display.stat,
    })
}

pub(crate) fn patch_options(args: PatchArgs) -> MarkResult<mark_command::DiffOptions> {
    Ok(mark_command::DiffOptions {
        repo: args.repo.repo,
        source: patch_source(args.path)?,
        scope: mark_command::DiffScope::All,
        include_untracked: false,
        stat: args.display.stat,
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
