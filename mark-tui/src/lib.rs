mod annotation;
mod app;
mod controls;
mod editor;
mod event_reader;
mod keymap;
mod live_diff;
mod model;
mod render;
mod run;
mod runtime;
mod search;
mod selector;
mod static_pager;
mod syntax;
#[cfg(test)]
mod tests;
mod text_input;
mod theme;
mod toast;

pub use run::{
    benchmark_diff_view, run, run_diff, run_diff_with_live_updates,
    run_diff_with_live_updates_and_syntax,
};
pub use static_pager::{
    StaticPagerLayout, StaticPagerOptions, render_static_changeset, render_static_pager,
};
pub use theme::{DiffBenchmarkOptions, DiffBenchmarkReport, SyntaxBenchmarkReport};
