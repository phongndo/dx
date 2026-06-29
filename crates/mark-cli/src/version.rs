pub(crate) const CLI_VERSION: &str = match option_env!("MARK_CLI_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
