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

/// ユーザー向けエラーメッセージのフォーマット（機密情報をマスク）
pub fn user_error(category: &str, err: impl std::fmt::Display) -> String {
    let raw = format!("{}: {}", category, err);
    sanitize_error_message(&raw)
}

/// エラーメッセージから AWS アカウント ID（12桁数字）やアクセスキーをマスク
fn sanitize_error_message(msg: &str) -> String {
    let mut result = msg.to_string();
    // AWS アカウント ID (12桁の数字列)
    if let Ok(re) = regex::Regex::new(r"\b\d{12}\b") {
        result = re.replace_all(&result, "***").to_string();
    }
    // AWS アクセスキー (AKIA...)
    if let Ok(re) = regex::Regex::new(r"AKIA[0-9A-Z]{16}") {
        result = re.replace_all(&result, "***").to_string();
    }
    result
}
