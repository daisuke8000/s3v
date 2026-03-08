use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ratatui_image::picker::Picker;
use tokio::sync::mpsc;

use crate::app::App;
use crate::error::user_error;
use crate::event::Event;
use crate::preview::page_cache::PageCacheManager;
use crate::preview::pdf_worker::{PdfWorker, WorkerResponse};
use crate::s3::S3Client;

use super::dispatch_event;
use super::preview_state::PreviewState;

/// PDF プレビューの最大サイズ（200MB）
const PDF_PREVIEW_MAX_BYTES: u64 = 200 * 1024 * 1024;

/// PDF ダウンロード（S3 download + body.collect を spawn 内で実行）
pub(crate) fn start_pdf_download(
    preview: &mut PreviewState,
    s3_client: &S3Client,
    stream_tx: &mpsc::UnboundedSender<Event>,
    bucket: &str,
    key: &str,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    preview.stream_cancel = Some(Arc::clone(&cancel));
    let pdf_data_slot = Arc::clone(&preview.pdf_data_slot);
    let tx = stream_tx.clone();
    let client = s3_client.inner().clone();
    let bucket = bucket.to_string();
    let key = key.to_string();

    tokio::spawn(async move {
        let output = match client.get_object().bucket(&bucket).key(&key).send().await {
            Ok(output) => output,
            Err(e) => {
                let _ = tx.send(Event::Error(user_error("S3 error", e)));
                return;
            }
        };

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        // サイズ制限チェック
        if let Some(len) = output.content_length().and_then(|l| u64::try_from(l).ok())
            && len > PDF_PREVIEW_MAX_BYTES
        {
            let _ = tx.send(Event::Error(format!(
                "PDF too large for preview ({:.1}MB, limit {:.0}MB)",
                len as f64 / 1_048_576.0,
                PDF_PREVIEW_MAX_BYTES as f64 / 1_048_576.0
            )));
            return;
        }

        match output.body.collect().await {
            Ok(bytes) => {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let pdf_bytes = bytes.into_bytes().to_vec();
                if let Ok(mut slot) = pdf_data_slot.lock() {
                    *slot = Some(pdf_bytes);
                }
                let _ = tx.send(Event::PdfDataReady);
            }
            Err(e) => {
                let _ = tx.send(Event::Error(user_error("PDF error", e)));
            }
        }
    });
}

/// PDF ワーカーセットアップ（メインループから呼び出し、Picker を使用するため !Send OK）
pub async fn setup_pdf_worker(app: &mut App, picker: &mut Picker, preview: &mut PreviewState) {
    let pdf_bytes = match preview.take_pdf_data() {
        Some(data) => data,
        None => return,
    };

    preview.pdf_raw_bytes = Some(pdf_bytes.clone());
    let mut worker = PdfWorker::spawn(pdf_bytes);

    match tokio::time::timeout(Duration::from_secs(5), worker.response_rx.recv()).await {
        Ok(Some(WorkerResponse::InitComplete {
            total_pages,
            first_page,
        })) => {
            preview.image_state = Some(picker.new_resize_protocol(first_page));
            preview.last_pdf_page = Some(0);
            preview.cache_manager = Some(PageCacheManager::new(total_pages));
            preview.worker = Some(worker);
            dispatch_event(
                app,
                Event::PreviewLoaded(crate::preview::PreviewContent::Pdf {
                    current_page: 0,
                    total_pages,
                }),
            );
        }
        Ok(Some(WorkerResponse::Error { error, .. })) => {
            dispatch_event(app, Event::Error(user_error("PDF error", error)));
        }
        Ok(None) => {
            dispatch_event(app, Event::Error("PDF worker channel closed".to_string()));
        }
        Ok(Some(_)) => {
            dispatch_event(
                app,
                Event::Error("Unexpected PDF worker response".to_string()),
            );
        }
        Err(_) => {
            dispatch_event(app, Event::Error("PDF init timeout".to_string()));
        }
    }
}

/// PDF ページ変更を検知してキャッシュベースで切替
pub async fn update_pdf_page(app: &mut App, preview: &mut PreviewState, picker: &mut Picker) {
    // ワーカーレスポンスをノンブロッキングで処理（先読み結果の回収）
    preview.process_worker_responses(picker);

    // ページ変更検知
    if let Some(crate::preview::PreviewContent::Pdf {
        current_page,
        total_pages: _,
    }) = &app.preview_content
    {
        let current = *current_page;
        let old = preview.last_pdf_page;

        if old == Some(current) {
            return;
        }

        // キャッシュヒット: old ページを cache に戻し、new ページを cache から取得
        if let Some(old_page) = old
            && preview.switch_page(old_page, current)
        {
            preview.update_cache_window(current);
            return;
        }

        // キャッシュミス: ワーカーに優先リクエスト送信
        if let Some(ref worker) = preview.worker {
            worker.request_page(current);
        } else {
            return;
        }

        // ワーカーからレスポンスを待機（take/put パターンで borrow 競合を回避）
        let mut worker = match preview.worker.take() {
            Some(w) => w,
            None => return,
        };

        let found = match tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                match worker.response_rx.recv().await {
                    Some(WorkerResponse::PageRendered { page, image }) => {
                        let protocol = picker.new_resize_protocol(image);
                        if page == current {
                            preview.image_state = Some(protocol);
                            preview.last_pdf_page = Some(current);
                            return true;
                        }
                        preview.page_cache.insert(page, protocol);
                    }
                    Some(WorkerResponse::Error { error, .. }) => {
                        dispatch_event(app, Event::Error(user_error("PDF error", error)));
                        return false;
                    }
                    None => {
                        dispatch_event(app, Event::Error("PDF worker channel closed".to_string()));
                        return false;
                    }
                    _ => {}
                }
            }
        })
        .await
        {
            Ok(found) => found,
            Err(_) => {
                dispatch_event(app, Event::Error("PDF render timeout".to_string()));
                false
            }
        };

        preview.worker = Some(worker);

        if found {
            preview.update_cache_window(current);
        }
    }
}
