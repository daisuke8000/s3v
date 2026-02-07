use thiserror::Error;

#[derive(Debug, Error)]
pub enum S3vError {
    #[error("AWS SDK error: {0}")]
    AwsSdk(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Terminal error: {0}")]
    Terminal(String),
}

pub type Result<T> = std::result::Result<T, S3vError>;
