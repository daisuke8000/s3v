pub mod image;
pub mod page_cache;
pub mod pdf;
pub mod pdf_worker;
pub mod text;

/// プレビュー可能なコンテンツの種類
#[derive(Debug, Clone)]
pub enum PreviewContent {
    Text(String),
    Image,
    Pdf {
        current_page: usize,
        total_pages: usize,
    },
    /// テキストストリーミング中（部分テキスト + ファイルキー）
    StreamingText {
        partial_text: String,
        key: String,
    },
    /// 画像ダウンロード中（プログレス表示）
    Downloading {
        received: u64,
        total: Option<u64>,
    },
}

/// ファイル名が指定された拡張子リストのいずれかに一致するか判定
pub fn has_extension(name: &str, extensions: &[&str]) -> bool {
    let lower = name.to_lowercase();
    extensions
        .iter()
        .any(|ext| lower.ends_with(&format!(".{}", ext)))
}

/// ファイルがプレビュー可能か判定（テキスト・画像・PDF のいずれか）
pub fn is_previewable_file(name: &str) -> bool {
    text::is_previewable(name) || image::is_image(name) || pdf::is_pdf(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_extension() {
        assert!(has_extension("test.json", &["json", "xml"]));
        assert!(has_extension("TEST.JSON", &["json"]));
        assert!(!has_extension("test.txt", &["json"]));
        assert!(!has_extension("test", &["json"]));
    }

    #[test]
    fn test_is_previewable_file() {
        assert!(is_previewable_file("readme.md"));
        assert!(is_previewable_file("photo.png"));
        assert!(is_previewable_file("doc.pdf"));
        assert!(!is_previewable_file("archive.zip"));
    }
}
