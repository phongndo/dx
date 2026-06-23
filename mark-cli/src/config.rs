use crate::{CliResult, write_stdout};

pub(crate) fn config() -> CliResult<()> {
    write_stdout(format_args!("{}\n", mark_command::config_path()?.display()))?;
    Ok(())
}
