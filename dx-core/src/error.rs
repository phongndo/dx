use std::{fmt, io};

pub type DxResult<T> = Result<T, DxError>;

#[derive(Debug)]
pub enum DxError {
    Io(io::Error),
    Json(serde_json::Error),
    Usage(String),
}

impl fmt::Display for DxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Usage(message) => write!(formatter, "{message}"),
        }
    }
}

impl From<io::Error> for DxError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DxError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
