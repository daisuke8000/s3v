pub mod image;
pub mod pdf;
pub mod text;

/// プレビュー可能なコンテンツの種類
#[derive(Debug, Clone)]
pub enum PreviewContent {
    Text(String),
    Image(Vec<u8>),
    Pdf {
        /// 各ページの PNG bytes（現在のページのみ保持）
        pages: Vec<Vec<u8>>,
        current_page: usize,
        total_pages: usize,
    },
}
