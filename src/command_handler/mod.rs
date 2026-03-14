mod image_download;
mod pdf_handler;
mod preview_state;
mod text_download;

pub use preview_state::PreviewState;

use crate::app::App;
use crate::command::Command;
use crate::event::Event;
use crate::s3::S3Client;

use tokio::sync::mpsc;

pub use pdf_handler::{setup_pdf_worker, update_pdf_page};

/// App にイベントを送信し、状態を更新するヘルパー
pub fn dispatch_event(app: &mut App, event: Event) -> Vec<Command> {
    let (new_app, cmds) = std::mem::take(app).handle_event(event);
    *app = new_app;
    cmds
}

/// LoadPreview コマンドの処理（非ブロッキング版: 即座にリターン）
///
/// 拡張子でファイル種別を判定し、S3 ダウンロードを含む全 I/O を tokio::spawn に委譲する。
pub fn start_preview_load(
    app: &mut App,
    s3_client: &S3Client,
    preview: &mut PreviewState,
    bucket: &str,
    key: &str,
    stream_tx: &mpsc::UnboundedSender<Event>,
) {
    // 前回のストリーミングをキャンセル
    preview.cancel_stream();

    if crate::preview::pdf::is_pdf(key) {
        pdf_handler::start_pdf_download(preview, s3_client, stream_tx, bucket, key);
    } else if crate::preview::image::is_image(key) {
        image_download::start_image_download(app, preview, s3_client, stream_tx, bucket, key);
    } else {
        text_download::start_text_download(app, preview, s3_client, stream_tx, bucket, key);
    }
}

/// プリフェッチ: テキストファイルの先頭部分を取得してキャッシュに格納
pub fn start_prefetch(
    s3_client: &S3Client,
    stream_tx: &mpsc::UnboundedSender<Event>,
    bucket: &str,
    key: &str,
) {
    text_download::start_prefetch_download(s3_client, stream_tx, bucket, key);
}

/// デバウンス付きプレビュー: フォルダまたはファイルのプレビュー要求を処理
pub fn start_debounced_preview(
    app: &mut App,
    s3_client: &S3Client,
    preview: &mut PreviewState,
    bucket: &str,
    key: &str,
    debounce_key: &str,
    stream_tx: &mpsc::UnboundedSender<Event>,
) {
    // デバウンスキーの一致確認
    if app.pending_preview_key.as_deref() != Some(debounce_key) {
        return;
    }

    if debounce_key.starts_with("folder:") {
        // フォルダプレビュー: list で子アイテムを取得
        let s3_client = s3_client.clone();
        let tx = stream_tx.clone();
        let bucket = bucket.to_string();
        let prefix = key.to_string();
        tokio::spawn(async move {
            let path = crate::s3::S3Path::with_prefix(&bucket, &prefix);
            match s3_client.list(&path).await {
                Ok(result) => {
                    let _ = tx.send(Event::FolderPreviewLoaded(result.items));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(crate::error::user_error(
                        "Folder preview failed",
                        e,
                    )));
                }
            }
        });
    } else if debounce_key.starts_with("bucket:") {
        // バケットプレビュー: バケット内のアイテムを取得
        let s3_client = s3_client.clone();
        let tx = stream_tx.clone();
        let bucket_name = bucket.to_string();
        tokio::spawn(async move {
            let path = crate::s3::S3Path::bucket(&bucket_name);
            match s3_client.list(&path).await {
                Ok(result) => {
                    let _ = tx.send(Event::FolderPreviewLoaded(result.items));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(crate::error::user_error(
                        "Bucket preview failed",
                        e,
                    )));
                }
            }
        });
    } else {
        // ファイルプレビュー
        start_preview_load(app, s3_client, preview, bucket, key, stream_tx);
    }
}
