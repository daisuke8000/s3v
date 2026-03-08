use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use crate::app::App;
use crate::command::Command;
use crate::event::Event;
use crate::preview::page_cache::PageCacheManager;
use crate::preview::pdf_worker::{PdfWorker, WorkerResponse};
use crate::preview::text::find_valid_utf8_boundary;
use crate::s3::S3Client;

/// ストリーミング読み込みのバッファサイズ（64KB）
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// プレビュー描画用のランタイム状態（App に入れられない non-Clone 型を管理）
pub struct PreviewState {
    pub image_state: Option<StatefulProtocol>,
    pub page_cache: HashMap<usize, StatefulProtocol>,
    pub cache_manager: Option<PageCacheManager>,
    pub pdf_raw_bytes: Option<Vec<u8>>,
    pub last_pdf_page: Option<usize>,
    worker: Option<PdfWorker>,
    /// ストリーミングキャンセル用フラグ
    stream_cancel: Option<Arc<AtomicBool>>,
    /// デコード済み画像の共有スロット（spawned task → main loop）
    image_slot: Arc<std::sync::Mutex<Option<image::DynamicImage>>>,
    /// PDF バイトデータの共有スロット（spawned task → main loop）
    pdf_data_slot: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewState {
    pub fn new() -> Self {
        Self {
            image_state: None,
            page_cache: HashMap::new(),
            cache_manager: None,
            pdf_raw_bytes: None,
            last_pdf_page: None,
            worker: None,
            stream_cancel: None,
            image_slot: Arc::new(std::sync::Mutex::new(None)),
            pdf_data_slot: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// デコード済み画像を取り出す（main loop 側で picker 処理に使う）
    pub fn take_decoded_image(&self) -> Option<image::DynamicImage> {
        self.image_slot.lock().ok().and_then(|mut slot| slot.take())
    }

    /// ダウンロード済み PDF バイトデータを取り出す（main loop 側で PdfWorker セットアップに使う）
    pub fn take_pdf_data(&self) -> Option<Vec<u8>> {
        self.pdf_data_slot
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
    }

    pub fn clear(&mut self) {
        // キャンセルフラグをセットしてストリーミングタスクを停止
        if let Some(cancel) = self.stream_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.image_state = None;
        self.page_cache.clear();
        self.cache_manager = None;
        self.pdf_raw_bytes = None;
        self.last_pdf_page = None;
        // image_slot をクリア
        if let Ok(mut slot) = self.image_slot.lock() {
            *slot = None;
        }
        // pdf_data_slot をクリア
        if let Ok(mut slot) = self.pdf_data_slot.lock() {
            *slot = None;
        }
        // PdfWorker::drop が Shutdown 送信 + join を行う
        self.worker = None;
    }

    /// ワーカーからのレスポンスをノンブロッキングで処理し、キャッシュに格納
    fn process_worker_responses(&mut self, picker: &mut Picker) {
        let mut worker = match self.worker.take() {
            Some(w) => w,
            None => return,
        };
        while let Some(resp) = worker.try_recv() {
            match resp {
                WorkerResponse::PageRendered { page, image } => {
                    self.page_cache
                        .insert(page, picker.new_resize_protocol(image));
                }
                WorkerResponse::Error { .. } => {}
                WorkerResponse::InitComplete { .. } => {}
            }
        }
        self.worker = Some(worker);
    }

    /// 旧ページをキャッシュに戻し、新ページをキャッシュから取得して image_state にセット
    fn switch_page(&mut self, old_page: usize, new_page: usize) -> bool {
        if let Some(new_protocol) = self.page_cache.remove(&new_page) {
            if let Some(old_protocol) = self.image_state.take() {
                self.page_cache.insert(old_page, old_protocol);
            }
            self.image_state = Some(new_protocol);
            self.last_pdf_page = Some(new_page);
            true
        } else {
            false
        }
    }

    /// キャッシュウィンドウを更新（エビクション + 先読みリクエスト）
    fn update_cache_window(&mut self, current_page: usize) {
        let cache_manager = match self.cache_manager.as_ref() {
            Some(cm) => cm,
            None => return,
        };

        let mut cached: HashSet<usize> = self.page_cache.keys().copied().collect();
        if let Some(p) = self.last_pdf_page {
            cached.insert(p);
        }

        let (to_request, to_evict) = cache_manager.update_window(current_page, &cached);

        for page in to_evict {
            self.page_cache.remove(&page);
        }

        if let Some(ref worker) = self.worker {
            worker.request_batch(to_request);
        }
    }
}

/// App にイベントを送信し、状態を更新するヘルパー
pub fn dispatch_event(app: &mut App, event: Event) -> Option<Command> {
    let (new_app, cmd) = std::mem::take(app).handle_event(event);
    *app = new_app;
    cmd
}

/// PDF ページ変更を検知してキャッシュベースで切替
pub async fn update_pdf_page(app: &mut App, preview: &mut PreviewState, picker: &mut Picker) {
    // ① ワーカーレスポンスをノンブロッキングで処理（先読み結果の回収）
    preview.process_worker_responses(picker);

    // ② ページ変更検知
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
                        dispatch_event(app, Event::Error(format!("PDF render error: {error}")));
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
    if let Some(cancel) = preview.stream_cancel.take() {
        cancel.store(true, Ordering::Relaxed);
    }

    if crate::preview::pdf::is_pdf(key) {
        start_pdf_download(preview, s3_client, stream_tx, bucket, key);
    } else if crate::preview::image::is_image(key) {
        start_image_download(app, preview, s3_client, stream_tx, bucket, key);
    } else {
        start_text_download(app, preview, s3_client, stream_tx, bucket, key);
    }
}

/// テキストプレビュー（S3 ダウンロード + ストリーミングを全て spawn 内で実行）
fn start_text_download(
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
    app.mode = crate::app::Mode::Preview;

    tokio::spawn(async move {
        let output = match client.get_object().bucket(&bucket).key(&key).send().await {
            Ok(output) => output,
            Err(e) => {
                let _ = tx.send(Event::Error(e.to_string()));
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
                    let _ = tx.send(Event::Error(format!("Stream read error: {}", e)));
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

/// 画像プレビュー（S3 ダウンロード + デコードを全て spawn 内で実行）
fn start_image_download(
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
                let _ = tx.send(Event::Error(e.to_string()));
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
                    let _ = tx.send(Event::Error(format!("Image download error: {}", e)));
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
                // 共有スロットにデコード済み画像をセット
                if let Ok(mut slot) = image_slot.lock() {
                    *slot = Some(dyn_img);
                }
                let _ = tx.send(Event::PreviewImageReady);
            }
            Ok(Err(e)) => {
                let _ = tx.send(Event::Error(format!("Image decode error: {}", e)));
            }
            Err(e) => {
                let _ = tx.send(Event::Error(format!("Image decode task error: {}", e)));
            }
        }
    });
}

/// PDF ダウンロード（S3 download + body.collect を spawn 内で実行、完了後 PdfDataReady 送信）
fn start_pdf_download(
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
                let _ = tx.send(Event::Error(e.to_string()));
                return;
            }
        };

        if cancel.load(Ordering::Relaxed) {
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
                let _ = tx.send(Event::Error(format!("PDF download error: {}", e)));
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
            dispatch_event(app, Event::Error(error));
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
