use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use crate::app::App;
use crate::error::user_error;
use crate::event::Event;
use crate::preview::text::find_valid_utf8_boundary;
use crate::s3::S3Client;

use super::PreviewState;

/// ストリーミング読み込みのバッファサイズ（64KB）
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// テキストプレビューの最大取得サイズ（128KB）
const PREVIEW_MAX_BYTES: usize = 128 * 1024;

/// テキストプレビュー（S3 ダウンロード + ストリーミングを全て spawn 内で実行）
pub(crate) fn start_text_download(
    app: &mut App,
    preview: &mut PreviewState,
    s3_client: &S3Client,
    stream_tx: &mpsc::UnboundedSender<Event>,
    bucket: &str,
    key: &str,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    preview.stream_cancel = Some(Arc::clone(&cancel));
    let tx = stream_tx.clone();
    let file_key = key.to_string();
    let client = s3_client.inner().clone();
    let bucket = bucket.to_string();
    let key = key.to_string();

    // StreamingText の初期状態をセット
    app.preview_content = Some(crate::preview::PreviewContent::StreamingText {
        partial_text: String::new(),
        key: file_key.clone(),
    });

    tokio::spawn(async move {
        let output = match client
            .get_object()
            .bucket(&bucket)
            .key(&key)
            .range(format!("bytes=0-{}", PREVIEW_MAX_BYTES - 1))
            .send()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                let _ = tx.send(Event::Error(user_error("S3 error", e)));
                return;
            }
        };

        let mut reader = output.body.into_async_read();
        let mut buf = vec![0u8; STREAM_BUFFER_SIZE];
        let mut remainder = Vec::new();
        let mut all_text = String::new();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let mut chunk_bytes = Vec::with_capacity(remainder.len() + n);
                    chunk_bytes.extend_from_slice(&remainder);
                    chunk_bytes.extend_from_slice(&buf[..n]);

                    let valid_end = find_valid_utf8_boundary(&chunk_bytes);
                    remainder = chunk_bytes[valid_end..].to_vec();

                    if valid_end > 0 {
                        let text = String::from_utf8_lossy(&chunk_bytes[..valid_end]).to_string();
                        all_text.push_str(&text);
                        let _ = tx.send(Event::PreviewChunk(text));
                    }
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(user_error("Stream error", e)));
                    return;
                }
            }
        }

        // 残りバイトがあれば処理
        if !remainder.is_empty() {
            let text = String::from_utf8_lossy(&remainder).to_string();
            all_text.push_str(&text);
            let _ = tx.send(Event::PreviewChunk(text));
        }

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        // 全受信後: 整形テキストを生成して StreamComplete
        let formatted = crate::preview::text::format_preview(&all_text, &file_key);
        let _ = tx.send(Event::PreviewStreamComplete(Some(formatted)));
    });
}

/// プリフェッチ用テキストダウンロード（先頭128KBのみ取得、キャッシュに格納）
pub(crate) fn start_prefetch_download(
    s3_client: &S3Client,
    stream_tx: &mpsc::UnboundedSender<Event>,
    bucket: &str,
    key: &str,
) {
    let tx = stream_tx.clone();
    let client = s3_client.inner().clone();
    let bucket = bucket.to_string();
    let file_key = key.to_string();

    tokio::spawn(async move {
        let output = match client
            .get_object()
            .bucket(&bucket)
            .key(&file_key)
            .range(format!("bytes=0-{}", PREVIEW_MAX_BYTES - 1))
            .send()
            .await
        {
            Ok(output) => output,
            Err(_) => return, // プリフェッチ失敗は黙殺
        };

        if let Ok(bytes) = output.body.collect().await {
            let raw = bytes.into_bytes();
            let text = String::from_utf8_lossy(&raw).to_string();
            let formatted = crate::preview::text::format_preview(&text, &file_key);
            let _ = tx.send(Event::PrefetchComplete {
                key: file_key,
                content: formatted,
            });
        }
    });
}
