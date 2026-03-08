pub mod text;

/// プレビュー可能なコンテンツの種類
#[derive(Debug, Clone)]
pub enum PreviewContent {
    Text(String),
}
