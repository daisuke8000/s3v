use arboard::Clipboard;

use crate::error::{Result, S3vError};

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard =
        Clipboard::new().map_err(|e| S3vError::Terminal(format!("Clipboard error: {}", e)))?;

    clipboard
        .set_text(text.to_string())
        .map_err(|e| S3vError::Terminal(format!("Clipboard error: {}", e)))?;

    Ok(())
}
