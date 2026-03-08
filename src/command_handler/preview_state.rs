use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

use crate::preview::page_cache::PageCacheManager;
use crate::preview::pdf_worker::{PdfWorker, WorkerResponse};

/// プレビュー描画用のランタイム状態（App に入れられない non-Clone 型を管理）
pub struct PreviewState {
    pub image_state: Option<StatefulProtocol>,
    pub page_cache: HashMap<usize, StatefulProtocol>,
    pub cache_manager: Option<PageCacheManager>,
    pub pdf_raw_bytes: Option<Vec<u8>>,
    pub last_pdf_page: Option<usize>,
    pub(crate) worker: Option<PdfWorker>,
    /// ストリーミングキャンセル用フラグ
    pub(crate) stream_cancel: Option<Arc<AtomicBool>>,
    /// デコード済み画像の共有スロット（spawned task → main loop）
    pub(crate) image_slot: Arc<std::sync::Mutex<Option<image::DynamicImage>>>,
    /// PDF バイトデータの共有スロット（spawned task → main loop）
    pub(crate) pdf_data_slot: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
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

    /// ダウンロード済み PDF バイトデータを取り出す
    pub fn take_pdf_data(&self) -> Option<Vec<u8>> {
        self.pdf_data_slot
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
    }

    /// 前回のストリーミングをキャンセル
    pub fn cancel_stream(&mut self) {
        if let Some(cancel) = self.stream_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn clear(&mut self) {
        self.cancel_stream();
        self.image_state = None;
        self.page_cache.clear();
        self.cache_manager = None;
        self.pdf_raw_bytes = None;
        self.last_pdf_page = None;
        if let Ok(mut slot) = self.image_slot.lock() {
            *slot = None;
        }
        if let Ok(mut slot) = self.pdf_data_slot.lock() {
            *slot = None;
        }
        // PdfWorker::drop が Shutdown 送信 + join を行う
        self.worker = None;
    }

    /// ワーカーからのレスポンスをノンブロッキングで処理し、キャッシュに格納
    pub(crate) fn process_worker_responses(&mut self, picker: &mut Picker) {
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
    pub(crate) fn switch_page(&mut self, old_page: usize, new_page: usize) -> bool {
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
    pub(crate) fn update_cache_window(&mut self, current_page: usize) {
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
