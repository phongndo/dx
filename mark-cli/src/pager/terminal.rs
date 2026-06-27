use std::{
    fs::OpenOptions,
    io::{self, IsTerminal},
};

#[cfg(unix)]
use std::os::fd::OwnedFd;

pub(super) fn sanitized_terminal_bytes(input: &[u8]) -> Vec<u8> {
    let stripped = strip_terminal_escapes(input);
    let text = String::from_utf8_lossy(&stripped);
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() && !matches!(character, '\n' | '\t') {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output.into_bytes()
}

pub(super) fn strip_terminal_escapes(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            0x1b => {
                if let Some(end) = escape_end(input, index) {
                    index = end;
                } else {
                    output.push(input[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    output
}

fn escape_end(input: &[u8], index: usize) -> Option<usize> {
    let introducer = input.get(index + 1).copied()?;
    let payload = index + 2;
    match introducer {
        b'[' => csi_escape_end(input, payload),
        b']' | b'P' | b'^' | b'_' | b'X' => string_escape_end(input, payload),
        0x20..=0x2f => input
            .get(payload)
            .filter(|byte| (0x30..=0x7e).contains(*byte))
            .map(|_| payload + 1),
        0x30..=0x7e => Some(payload),
        _ => None,
    }
}

pub(super) fn csi_escape_end(input: &[u8], mut index: usize) -> Option<usize> {
    let mut seen_intermediate = false;
    while let Some(byte) = input.get(index).copied() {
        match byte {
            0x30..=0x3f if !seen_intermediate => index += 1,
            0x20..=0x2f => {
                seen_intermediate = true;
                index += 1;
            }
            0x40..=0x7e => return Some(index + 1),
            _ => return None,
        }
    }
    None
}

fn string_escape_end(input: &[u8], mut index: usize) -> Option<usize> {
    while let Some(byte) = input.get(index).copied() {
        match byte {
            0x07 => return Some(index + 1),
            b'\n' | b'\r' => return None,
            0x1b if input.get(index + 1) == Some(&b'\\') => return Some(index + 2),
            0x1b => return None,
            _ => index += 1,
        }
    }
    None
}

#[cfg(unix)]
pub(super) fn controlling_terminal_available() -> bool {
    OpenOptions::new().read(true).open("/dev/tty").is_ok()
}

#[cfg(not(unix))]
pub(super) fn controlling_terminal_available() -> bool {
    false
}

#[cfg(unix)]
pub(super) fn attach_controlling_terminal_to_stdin() -> io::Result<Option<StdinOverride>> {
    if io::stdin().is_terminal() {
        return Ok(None);
    }

    let stdin = io::stdin();
    let original = rustix::io::dup(&stdin).map_err(io::Error::from)?;
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    rustix::stdio::dup2_stdin(&tty).map_err(io::Error::from)?;
    Ok(Some(StdinOverride { original }))
}

#[cfg(not(unix))]
pub(super) fn attach_controlling_terminal_to_stdin() -> io::Result<Option<StdinOverride>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "attaching redirected pager stdin to the controlling terminal is unsupported on this platform",
    ))
}

#[cfg(unix)]
pub(super) struct StdinOverride {
    original: OwnedFd,
}

#[cfg(unix)]
impl Drop for StdinOverride {
    fn drop(&mut self) {
        let _ = rustix::stdio::dup2_stdin(&self.original);
    }
}

#[cfg(not(unix))]
pub(super) struct StdinOverride;
