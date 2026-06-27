use std::{fmt, io};

pub type MarkResult<T> = Result<T, MarkError>;

#[derive(Debug)]
pub enum MarkError {
    Io(io::Error),
    Json(serde_json::Error),
    Usage(String),
}

impl fmt::Display for MarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Usage(message) => write!(formatter, "{message}"),
        }
    }
}

impl From<io::Error> for MarkError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for MarkError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
