use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Every failure mode the engine can surface to a caller.
#[derive(Debug, Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// The server ignored our `Range` request, so the transfer cannot be
    /// segmented and must run as a single stream.
    #[error("server does not support range requests")]
    NoRangeSupport,

    #[error("download was cancelled")]
    Cancelled,

    /// Internal control-flow signal: the transfer was paused and may resume.
    #[error("download was paused")]
    Paused,

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("no download with id {0}")]
    NotFound(String),

    #[error("{0}")]
    Internal(String),
}

impl Error {
    pub fn internal(msg: impl Into<String>) -> Self {
        Error::Internal(msg.into())
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Error::Invalid(msg.into())
    }
}
