use std::{env, ffi::OsString};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PagerEnv {
    pub(super) term: Option<OsString>,
    pub(super) lv: Option<OsString>,
    pub(super) git_pager: Option<OsString>,
    pub(super) has_lazygit_env: bool,
}

impl PagerEnv {
    pub(super) fn current() -> Self {
        Self {
            term: env::var_os("TERM"),
            lv: env::var_os("LV"),
            git_pager: env::var_os("GIT_PAGER"),
            has_lazygit_env: env::vars_os()
                .any(|(key, _)| key.to_string_lossy().starts_with("LAZYGIT")),
        }
    }

    pub(super) fn term_is_dumb(&self) -> bool {
        self.term.as_deref() == Some(std::ffi::OsStr::new("dumb"))
    }

    pub(super) fn is_captured_pager_host(&self) -> bool {
        self.term_is_dumb()
            && (self.lv.as_deref() == Some(std::ffi::OsStr::new("-c"))
                || self.git_pager.is_some()
                || self.has_lazygit_env)
    }
}
