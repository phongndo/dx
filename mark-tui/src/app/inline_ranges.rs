use super::DiffApp;
use crate::syntax::{InlineHunkEmphasisCache, InlineHunkKey, InlineRange};

impl DiffApp {
    pub(crate) fn inline_ranges(
        &mut self,
        file: usize,
        hunk: usize,
        line: usize,
    ) -> Vec<InlineRange> {
        let key = InlineHunkKey {
            generation: self.document.generation,
            file,
            hunk,
        };
        if !self.document.inline_cache.contains_key(&key) {
            let cache = self
                .document
                .changeset
                .files
                .get(file)
                .and_then(|file_diff| file_diff.hunks.get(hunk))
                .map(|hunk_diff| InlineHunkEmphasisCache::new(&hunk_diff.lines))
                .unwrap_or_else(|| InlineHunkEmphasisCache::new(&[]));
            self.document.inline_cache.insert(key, cache);
        }

        let Some(lines) = self
            .document
            .changeset
            .files
            .get(file)
            .and_then(|file_diff| file_diff.hunks.get(hunk))
            .map(|hunk_diff| hunk_diff.lines.as_slice())
        else {
            return Vec::new();
        };

        self.document
            .inline_cache
            .get_mut(&key)
            .map(|hunk_emphasis| hunk_emphasis.ranges_for_line(lines, line))
            .unwrap_or_default()
    }
}
