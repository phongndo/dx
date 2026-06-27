use mark_diff::{DiffLine, DiffLineKind};

use crate::theme::{MAX_INLINE_DIFF_LINE_BYTES, MAX_INLINE_DIFF_TOKENS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InlineHunkKey {
    pub(crate) generation: u64,
    pub(crate) file: usize,
    pub(crate) hunk: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InlineRange {
    pub(crate) byte_start: usize,
    pub(crate) byte_end: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InlineLineEmphasis {
    pub(crate) ranges: Vec<InlineRange>,
}

#[derive(Debug)]
pub(crate) struct InlineHunkEmphasisCache {
    pub(crate) lines: Vec<Option<InlineLineEmphasis>>,
    pub(crate) blocks: Vec<InlineChangedBlock>,
}

#[derive(Debug)]
pub(crate) struct InlineChangedBlock {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) deletions: Vec<usize>,
    pub(crate) additions: Vec<usize>,
}

impl InlineHunkEmphasisCache {
    pub(crate) fn new(lines: &[DiffLine]) -> Self {
        let mut blocks = Vec::new();
        let mut index = 0usize;

        while index < lines.len() {
            if !matches!(
                lines[index].kind,
                DiffLineKind::Deletion | DiffLineKind::Addition
            ) {
                index += 1;
                continue;
            }

            let start = index;
            let mut deletions = Vec::new();
            let mut additions = Vec::new();
            while index < lines.len()
                && matches!(
                    lines[index].kind,
                    DiffLineKind::Deletion | DiffLineKind::Addition
                )
            {
                match lines[index].kind {
                    DiffLineKind::Deletion => deletions.push(index),
                    DiffLineKind::Addition => additions.push(index),
                    DiffLineKind::Context | DiffLineKind::Meta => {}
                }
                index += 1;
            }
            blocks.push(InlineChangedBlock {
                start,
                end: index,
                deletions,
                additions,
            });
        }

        Self {
            lines: vec![None; lines.len()],
            blocks,
        }
    }

    pub(crate) fn ranges_for_line(&mut self, lines: &[DiffLine], line: usize) -> Vec<InlineRange> {
        if let Some(Some(emphasis)) = self.lines.get(line) {
            return emphasis.ranges.clone();
        }

        self.compute_line(lines, line);
        self.lines
            .get(line)
            .and_then(|emphasis| emphasis.as_ref())
            .map(|emphasis| emphasis.ranges.clone())
            .unwrap_or_default()
    }

    pub(crate) fn compute_line(&mut self, lines: &[DiffLine], line: usize) {
        let Some(diff_line) = lines.get(line) else {
            return;
        };
        if !matches!(
            diff_line.kind,
            DiffLineKind::Deletion | DiffLineKind::Addition
        ) {
            self.set_emphasis(line, Vec::new());
            return;
        }

        let Some(block) = self
            .blocks
            .iter()
            .find(|block| line >= block.start && line < block.end)
        else {
            self.set_emphasis(line, Vec::new());
            return;
        };

        if block.deletions.is_empty() || block.additions.is_empty() {
            let (start, end) = (block.start, block.end);
            for line in start..end {
                self.set_emphasis(line, Vec::new());
            }
            return;
        }

        let (old_index, new_index) = match diff_line.kind {
            DiffLineKind::Deletion => {
                let Ok(pair_index) = block.deletions.binary_search(&line) else {
                    self.set_emphasis(line, Vec::new());
                    return;
                };
                let Some(new_index) = block.additions.get(pair_index).copied() else {
                    self.set_emphasis(line, Vec::new());
                    return;
                };
                (line, new_index)
            }
            DiffLineKind::Addition => {
                let Ok(pair_index) = block.additions.binary_search(&line) else {
                    self.set_emphasis(line, Vec::new());
                    return;
                };
                let Some(old_index) = block.deletions.get(pair_index).copied() else {
                    self.set_emphasis(line, Vec::new());
                    return;
                };
                (old_index, line)
            }
            DiffLineKind::Context | DiffLineKind::Meta => unreachable!(),
        };

        let (old_ranges, new_ranges) =
            changed_token_ranges(&lines[old_index].text, &lines[new_index].text);
        self.set_emphasis(old_index, old_ranges);
        self.set_emphasis(new_index, new_ranges);
    }

    pub(crate) fn set_emphasis(&mut self, line: usize, ranges: Vec<InlineRange>) {
        if let Some(emphasis) = self.lines.get_mut(line) {
            *emphasis = Some(InlineLineEmphasis { ranges });
        }
    }
}

#[cfg(test)]
pub(crate) fn compute_hunk_inline_emphasis(lines: &[DiffLine]) -> Vec<InlineLineEmphasis> {
    let mut emphasis = vec![InlineLineEmphasis::default(); lines.len()];
    let mut index = 0usize;

    while index < lines.len() {
        match lines[index].kind {
            DiffLineKind::Deletion | DiffLineKind::Addition => {
                let mut deletions = Vec::new();
                let mut additions = Vec::new();
                while index < lines.len()
                    && matches!(
                        lines[index].kind,
                        DiffLineKind::Deletion | DiffLineKind::Addition
                    )
                {
                    match lines[index].kind {
                        DiffLineKind::Deletion => deletions.push(index),
                        DiffLineKind::Addition => additions.push(index),
                        DiffLineKind::Context | DiffLineKind::Meta => {}
                    }
                    index += 1;
                }
                compute_changed_block_inline_emphasis(lines, &deletions, &additions, &mut emphasis);
            }
            DiffLineKind::Context | DiffLineKind::Meta => index += 1,
        }
    }

    emphasis
}

#[cfg(test)]
pub(crate) fn compute_changed_block_inline_emphasis(
    lines: &[DiffLine],
    deletions: &[usize],
    additions: &[usize],
    emphasis: &mut [InlineLineEmphasis],
) {
    let paired_rows = deletions.len().max(additions.len());
    for pair_index in 0..paired_rows {
        match (deletions.get(pair_index), additions.get(pair_index)) {
            (Some(deletion), Some(addition)) => {
                let (old_ranges, new_ranges) =
                    changed_token_ranges(&lines[*deletion].text, &lines[*addition].text);
                emphasis[*deletion].ranges = old_ranges;
                emphasis[*addition].ranges = new_ranges;
            }
            (Some(deletion), None) => {
                emphasis[*deletion].ranges = Vec::new();
            }
            (None, Some(addition)) => {
                emphasis[*addition].ranges = Vec::new();
            }
            (None, None) => {}
        }
    }
}

pub(crate) fn changed_token_ranges(old: &str, new: &str) -> (Vec<InlineRange>, Vec<InlineRange>) {
    if old == new {
        return (Vec::new(), Vec::new());
    }
    if old.len() > MAX_INLINE_DIFF_LINE_BYTES || new.len() > MAX_INLINE_DIFF_LINE_BYTES {
        return (Vec::new(), Vec::new());
    }

    let old_tokens = inline_tokens(old);
    let new_tokens = inline_tokens(new);
    if old_tokens.len() > MAX_INLINE_DIFF_TOKENS || new_tokens.len() > MAX_INLINE_DIFF_TOKENS {
        return (Vec::new(), Vec::new());
    }

    let mut old_changed = vec![true; old_tokens.len()];
    let mut new_changed = vec![true; new_tokens.len()];
    mark_unchanged_lcs_tokens(
        old,
        &old_tokens,
        new,
        &new_tokens,
        &mut old_changed,
        &mut new_changed,
    );

    (
        inline_ranges_from_tokens(&old_tokens, &old_changed),
        inline_ranges_from_tokens(&new_tokens, &new_changed),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InlineToken {
    pub(crate) byte_start: usize,
    pub(crate) byte_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineCharClass {
    Word,
    Whitespace,
    Other,
}

pub(crate) fn inline_tokens(text: &str) -> Vec<InlineToken> {
    if text.is_ascii() {
        return inline_tokens_ascii(text);
    }

    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        let class = inline_char_class(ch);
        let mut end = start + ch.len_utf8();

        if class != InlineCharClass::Other {
            while let Some((_, next)) = chars.peek().copied() {
                if inline_char_class(next) != class {
                    break;
                }
                let Some((next_start, next)) = chars.next() else {
                    break;
                };
                end = next_start + next.len_utf8();
            }
        }

        tokens.push(InlineToken {
            byte_start: start,
            byte_end: end,
        });
    }

    tokens
}

pub(crate) fn inline_tokens_ascii(text: &str) -> Vec<InlineToken> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut start = 0usize;

    while start < bytes.len() {
        let class = inline_ascii_class(bytes[start]);
        let mut end = start + 1;

        if class != InlineCharClass::Other {
            while end < bytes.len() && inline_ascii_class(bytes[end]) == class {
                end += 1;
            }
        }

        tokens.push(InlineToken {
            byte_start: start,
            byte_end: end,
        });
        start = end;
    }

    tokens
}

pub(crate) fn inline_ascii_class(byte: u8) -> InlineCharClass {
    if byte.is_ascii_whitespace() || byte == 0x0B {
        InlineCharClass::Whitespace
    } else if byte == b'_' || byte.is_ascii_alphanumeric() {
        InlineCharClass::Word
    } else {
        InlineCharClass::Other
    }
}

pub(crate) fn inline_char_class(ch: char) -> InlineCharClass {
    if ch.is_whitespace() {
        InlineCharClass::Whitespace
    } else if ch == '_' || ch.is_alphanumeric() {
        InlineCharClass::Word
    } else {
        InlineCharClass::Other
    }
}

pub(crate) fn mark_unchanged_lcs_tokens(
    old: &str,
    old_tokens: &[InlineToken],
    new: &str,
    new_tokens: &[InlineToken],
    old_changed: &mut [bool],
    new_changed: &mut [bool],
) {
    let cols = new_tokens.len() + 1;
    let mut lengths = vec![0u16; (old_tokens.len() + 1) * cols];

    for old_index in 0..old_tokens.len() {
        for new_index in 0..new_tokens.len() {
            let cell = (old_index + 1) * cols + new_index + 1;
            lengths[cell] = if inline_token_text(old, old_tokens[old_index])
                == inline_token_text(new, new_tokens[new_index])
            {
                lengths[old_index * cols + new_index].saturating_add(1)
            } else {
                lengths[old_index * cols + new_index + 1]
                    .max(lengths[(old_index + 1) * cols + new_index])
            };
        }
    }

    let mut old_index = old_tokens.len();
    let mut new_index = new_tokens.len();
    while old_index > 0 && new_index > 0 {
        if inline_token_text(old, old_tokens[old_index - 1])
            == inline_token_text(new, new_tokens[new_index - 1])
        {
            old_changed[old_index - 1] = false;
            new_changed[new_index - 1] = false;
            old_index -= 1;
            new_index -= 1;
        } else if lengths[(old_index - 1) * cols + new_index]
            >= lengths[old_index * cols + new_index - 1]
        {
            old_index -= 1;
        } else {
            new_index -= 1;
        }
    }
}

pub(crate) fn inline_token_text(text: &str, token: InlineToken) -> &str {
    &text[token.byte_start..token.byte_end]
}

pub(crate) fn inline_ranges_from_tokens(
    tokens: &[InlineToken],
    changed: &[bool],
) -> Vec<InlineRange> {
    let mut ranges: Vec<InlineRange> = Vec::new();
    for (token, is_changed) in tokens.iter().zip(changed) {
        if !*is_changed {
            continue;
        }
        if let Some(last) = ranges.last_mut()
            && last.byte_end == token.byte_start
        {
            last.byte_end = token.byte_end;
            continue;
        }
        ranges.push(InlineRange {
            byte_start: token.byte_start,
            byte_end: token.byte_end,
        });
    }
    ranges
}
