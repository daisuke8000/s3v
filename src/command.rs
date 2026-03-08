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
    /// アプリケーション終了
    Quit,
}
