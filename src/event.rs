use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::preview::PreviewContent;
use crate::s3::S3Item;

/// アプリケーションイベント
#[derive(Debug, Clone)]
pub enum Event {
    /// キー入力
    Key(KeyEvent),
    /// アイテム一覧の読み込み完了
    ItemsLoaded(Vec<S3Item>),
    /// プレビューの読み込み完了
    PreviewLoaded(PreviewContent),
    /// 検索結果
    SearchResults(Vec<S3Item>),
    /// メタデータインデックス完了
    MetadataIndexed(usize),
    /// テキストストリーミングチャンク受信
    PreviewChunk(String),
    /// ストリーミング完了（整形テキスト or None）
    PreviewStreamComplete(Option<String>),
    /// 画像ダウンロード進捗
    PreviewProgress { received: u64, total: Option<u64> },
    /// 画像デコード完了
    PreviewImageReady,
    /// PDF データダウンロード完了（メインループで PdfWorker セットアップ）
    PdfDataReady,
    /// 親ペインのアイテム一覧読み込み完了
    ParentItemsLoaded(Vec<S3Item>),
    /// フォルダプレビューの読み込み完了
    FolderPreviewLoaded(Vec<S3Item>),
    /// デバウンスタイマー満了
    DebounceTimeout { debounce_key: String },
    /// プリフェッチ完了（キャッシュに格納）
    PrefetchComplete { key: String, content: String },
    /// フォルダ内ファイル一覧取得完了
    FolderFilesListed { files: Vec<S3Item>, total_size: u64 },
    /// 個別ファイルDL完了（進捗更新）
    DownloadFileComplete {
        completed: usize,
        total: usize,
        current_file: String,
    },
    /// 全DL完了
    DownloadAllComplete { count: usize },
    /// エラー発生
    Error(String),
    /// 終了要求
    Quit,
}

impl Event {
    /// キーイベントから Event を生成
    pub fn from_key(key: KeyEvent) -> Self {
        // Ctrl+C は終了
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Event::Quit;
        }

        // 'q' は終了
        if key.code == KeyCode::Char('q') {
            return Event::Quit;
        }

        Event::Key(key)
    }
}
