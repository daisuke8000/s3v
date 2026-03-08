use std::path::PathBuf;

use crate::s3::S3Path;

/// 副作用を伴うコマンド
#[derive(Debug, Clone)]
pub enum Command {
    /// 指定パスのアイテム一覧を取得
    LoadItems(S3Path),
    /// ファイルをダウンロード
    Download {
        bucket: String,
        key: String,
        destination: PathBuf,
    },
    /// ファイルのプレビューを読み込み
    LoadPreview { bucket: String, key: String },
    /// メタデータインデックスを構築
    IndexMetadata { bucket: String },
    /// SQL 検索を実行
    ExecuteSearch(String),
    /// アプリケーション終了
    Quit,
    /// 親ペインのアイテム一覧を取得
    LoadParentItems(S3Path),
    /// フォルダプレビュー（右ペインにフォルダ内容を表示）
    LoadFolderPreview { bucket: String, prefix: String },
    /// デバウンス付きプレビュー要求（main.rs がタイマー管理）
    RequestPreview {
        bucket: String,
        key: String,
        debounce_key: String,
    },
    /// プリフェッチ要求（隣接アイテムの先読み、低優先度）
    PrefetchPreview { bucket: String, key: String },
    /// 現在のプレビュー読み込みをキャンセル
    CancelPreview,
}
