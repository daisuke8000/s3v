pub mod image;
pub mod pdf;
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
}

/// ファイル名が指定された拡張子リストのいずれかに一致するか判定
pub fn has_extension(name: &str, extensions: &[&str]) -> bool {
    let lower = name.to_lowercase();
    extensions
        .iter()
        .any(|ext| lower.ends_with(&format!(".{}", ext)))
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
}
