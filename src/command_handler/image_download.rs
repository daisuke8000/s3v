use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use crate::app::App;
use crate::error::user_error;
use crate::event::Event;
use crate::s3::S3Client;

use super::dispatch_event;
use super::preview_state::PreviewState;

/// ストリーミング読み込みのバッファサイズ（64KB）
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// 画像プレビュー（S3 ダウンロード + デコードを全て spawn 内で実行）
pub(crate) fn start_image_download(
    app: &mut App,
    preview: &mut PreviewState,
    s3_client: &S3Client,
    stream_tx: &mpsc::UnboundedSender<Event>,
    bucket: &str,
    key: &str,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    preview.stream_cancel = Some(Arc::clone(&cancel));
    let image_slot = Arc::clone(&preview.image_slot);
    let tx = stream_tx.clone();
    let client = s3_client.inner().clone();
    let bucket = bucket.to_string();
    let key = key.to_string();

    // 初期プログレス表示
    dispatch_event(
        app,
        Event::PreviewProgress {
            received: 0,
            total: None,
        },
    );

    tokio::spawn(async move {
        let output = match client.get_object().bucket(&bucket).key(&key).send().await {
            Ok(output) => output,
            Err(e) => {
                let _ = tx.send(Event::Error(user_error("S3 error", e)));
                return;
            }
        };

        let content_length = output.content_length().map(|l| l as u64);
        let mut reader = output.body.into_async_read();
        let mut all_bytes = Vec::new();
        let mut buf = vec![0u8; STREAM_BUFFER_SIZE];

        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    all_bytes.extend_from_slice(&buf[..n]);
                    let _ = tx.send(Event::PreviewProgress {
                        received: all_bytes.len() as u64,
                        total: content_length,
                    });
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(user_error("Image error", e)));
                    return;
                }
            }
        }

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        // CPU 集約のデコードをブロッキングスレッドで実行
        match tokio::task::spawn_blocking(move || image::load_from_memory(&all_bytes)).await {
            Ok(Ok(dyn_img)) => {
                if let Ok(mut slot) = image_slot.lock() {
                    *slot = Some(dyn_img);
                }
                let _ = tx.send(Event::PreviewImageReady);
            }
            Ok(Err(e)) => {
                let _ = tx.send(Event::Error(user_error("Image error", e)));
            }
            Err(e) => {
                let _ = tx.send(Event::Error(user_error("Image error", e)));
            }
        }
    });
}
