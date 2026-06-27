use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextInputKeyResult {
    Ignored,
    Handled,
    Moved,
    Edited,
}

pub(crate) fn handle_text_input_key(
    input: &mut String,
    cursor: &mut usize,
    key: KeyEvent,
) -> TextInputKeyResult {
    clamp_text_cursor(input, cursor);
    if input.is_empty() {
        match key.code {
            KeyCode::Home
            | KeyCode::End
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Backspace
            | KeyCode::Delete => return TextInputKeyResult::Ignored,
            KeyCode::Char('a' | 'e' | 'u' | 'k' | 'w')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                return TextInputKeyResult::Ignored;
            }
            _ => {}
        }
    }
    let before_input = input.clone();
    let before_cursor = *cursor;

    let handled = match key.code {
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *cursor = line_start(input, *cursor);
            true
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *cursor = line_end(input, *cursor);
            true
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_range(input, cursor, line_start(input, *cursor), *cursor);
            true
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_range(input, cursor, *cursor, line_end(input, *cursor));
            true
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_range(
                input,
                cursor,
                previous_word_boundary(input, *cursor),
                *cursor,
            );
            true
        }
        KeyCode::Home => {
            *cursor = line_start(input, *cursor);
            true
        }
        KeyCode::End => {
            *cursor = line_end(input, *cursor);
            true
        }
        KeyCode::Left if key_has_command_modifier(key.modifiers) => {
            *cursor = line_start(input, *cursor);
            true
        }
        KeyCode::Right if key_has_command_modifier(key.modifiers) => {
            *cursor = line_end(input, *cursor);
            true
        }
        KeyCode::Left if key_has_word_modifier(key.modifiers) => {
            *cursor = previous_word_boundary(input, *cursor);
            true
        }
        KeyCode::Right if key_has_word_modifier(key.modifiers) => {
            *cursor = next_word_boundary(input, *cursor);
            true
        }
        KeyCode::Left => {
            *cursor = previous_char_boundary(input, *cursor);
            true
        }
        KeyCode::Right => {
            *cursor = next_char_boundary(input, *cursor);
            true
        }
        KeyCode::Backspace | KeyCode::Delete if key_has_command_modifier(key.modifiers) => {
            delete_range(input, cursor, line_start(input, *cursor), *cursor);
            true
        }
        KeyCode::Backspace if key_has_word_modifier(key.modifiers) => {
            delete_range(
                input,
                cursor,
                previous_word_boundary(input, *cursor),
                *cursor,
            );
            true
        }
        KeyCode::Delete if key_has_word_modifier(key.modifiers) => {
            delete_range(input, cursor, *cursor, next_word_boundary(input, *cursor));
            true
        }
        KeyCode::Backspace => {
            delete_range(
                input,
                cursor,
                previous_char_boundary(input, *cursor),
                *cursor,
            );
            true
        }
        KeyCode::Delete => {
            delete_range(input, cursor, *cursor, next_char_boundary(input, *cursor));
            true
        }
        KeyCode::Char(character) if is_text_input_character(key.modifiers) => {
            input.insert(*cursor, character);
            *cursor += character.len_utf8();
            true
        }
        _ => false,
    };

    if !handled {
        TextInputKeyResult::Ignored
    } else if before_input != *input {
        TextInputKeyResult::Edited
    } else if before_cursor != *cursor {
        TextInputKeyResult::Moved
    } else {
        TextInputKeyResult::Handled
    }
}

fn is_text_input_character(modifiers: KeyModifiers) -> bool {
    !modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    )
}

fn key_has_command_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::META)
}

fn key_has_word_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
}

fn clamp_text_cursor(input: &str, cursor: &mut usize) {
    *cursor = (*cursor).min(input.len());
    while *cursor > 0 && !input.is_char_boundary(*cursor) {
        *cursor -= 1;
    }
}

fn delete_range(input: &mut String, cursor: &mut usize, start: usize, end: usize) {
    let start = start.min(input.len());
    let end = end.min(input.len());
    if start >= end || !input.is_char_boundary(start) || !input.is_char_boundary(end) {
        return;
    }
    input.replace_range(start..end, "");
    *cursor = start;
}

fn previous_char_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor.min(input.len())]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_char_boundary(input: &str, cursor: usize) -> usize {
    let cursor = cursor.min(input.len());
    input[cursor..]
        .chars()
        .next()
        .map(|character| cursor + character.len_utf8())
        .unwrap_or(cursor)
}

fn line_start(input: &str, cursor: usize) -> usize {
    input[..cursor.min(input.len())]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end(input: &str, cursor: usize) -> usize {
    let cursor = cursor.min(input.len());
    input[cursor..]
        .find('\n')
        .map(|index| cursor + index)
        .unwrap_or(input.len())
}

fn previous_word_boundary(input: &str, cursor: usize) -> usize {
    let mut index = cursor.min(input.len());
    while index > 0 {
        let prev = previous_char_boundary(input, index);
        let ch = input[prev..index].chars().next().unwrap_or_default();
        if !ch.is_whitespace() {
            break;
        }
        index = prev;
    }
    while index > 0 {
        let prev = previous_char_boundary(input, index);
        let ch = input[prev..index].chars().next().unwrap_or_default();
        if ch.is_whitespace() {
            break;
        }
        index = prev;
    }
    index
}

fn next_word_boundary(input: &str, cursor: usize) -> usize {
    let mut index = cursor.min(input.len());
    while index < input.len() {
        let next = next_char_boundary(input, index);
        let ch = input[index..next].chars().next().unwrap_or_default();
        if ch.is_whitespace() {
            break;
        }
        index = next;
    }
    while index < input.len() {
        let next = next_char_boundary(input, index);
        let ch = input[index..next].chars().next().unwrap_or_default();
        if !ch.is_whitespace() {
            break;
        }
        index = next;
    }
    index
}
