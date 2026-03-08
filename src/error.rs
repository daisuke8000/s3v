use thiserror::Error;

#[derive(Debug, Error)]
pub enum S3vError {
    #[error("AWS SDK error: {0}")]
    AwsSdk(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PDF error: {0}")]
    Pdf(String),

    #[error("Search error: {0}")]
    Search(String),

    #[error("Terminal error: {0}")]
    Terminal(String),
}

pub type Result<T> = std::result::Result<T, S3vError>;
