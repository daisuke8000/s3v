use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::event::Event;
use crate::s3::{S3Item, S3Path};

const CACHE_TTL: Duration = Duration::from_secs(60);
const ERROR_DISPLAY_DURATION: Duration = Duration::from_secs(3);

#[derive(Debug)]
struct CacheEntry {
    items: Vec<S3Item>,
    cached_at: Instant,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > CACHE_TTL
    }
}

/// メインループ専用の状態。App (Model) に入れるべきでない副作用的な状態を管理する。
/// TEA の純粋性を維持するため、Instant やキャッシュはここに配置する。
#[derive(Debug, Default)]
pub struct RuntimeState {
    cache: HashMap<String, CacheEntry>,
    error_shown_at: Option<Instant>,
    status_shown_at: Option<Instant>,
}

impl RuntimeState {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Cache methods ──

    pub fn get_cached(&self, path: &S3Path) -> Option<&Vec<S3Item>> {
        let key = path.to_s3_uri();
        self.cache.get(&key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(&entry.items)
            }
        })
    }

    pub fn set_cache(&mut self, path: &S3Path, items: Vec<S3Item>) {
        let key = path.to_s3_uri();
        self.cache.insert(
            key,
            CacheEntry {
                items,
                cached_at: Instant::now(),
            },
        );
    }

    // ── Message timer methods ──

    /// エラーメッセージの表示タイマーを開始
    pub fn mark_error_shown(&mut self) {
        self.error_shown_at = Some(Instant::now());
    }

    /// ステータスメッセージの表示タイマーを開始
    pub fn mark_status_shown(&mut self) {
        self.status_shown_at = Some(Instant::now());
    }

    /// エラー表示を自動消去すべきか。3秒経過で true を返しタイマーをリセット。
    pub fn should_dismiss_error(&mut self) -> bool {
        if let Some(shown_at) = self.error_shown_at
            && shown_at.elapsed() > ERROR_DISPLAY_DURATION
        {
            self.error_shown_at = None;
            return true;
        }
        false
    }

    /// ステータス表示を自動消去すべきか。3秒経過で true を返しタイマーをリセット。
    pub fn should_dismiss_status(&mut self) -> bool {
        if let Some(shown_at) = self.status_shown_at
            && shown_at.elapsed() > ERROR_DISPLAY_DURATION
        {
            self.status_shown_at = None;
            return true;
        }
        false
    }
}

// ── Debounce ──

/// デバウンス状態（プレビュー要求のタイマー管理）
#[derive(Default)]
pub struct DebounceState {
    pub pending_key: Option<String>,
    pub pending_bucket: String,
    pub pending_obj_key: String,
    timer_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DebounceState {
    pub fn new() -> Self {
        Self {
            pending_key: None,
            pending_bucket: String::new(),
            pending_obj_key: String::new(),
            timer_handle: None,
        }
    }

    /// 新しいデバウンス要求をセット（前のタイマーをキャンセル）
    pub fn schedule(
        &mut self,
        debounce_key: String,
        bucket: String,
        obj_key: String,
        debounce_tx: mpsc::UnboundedSender<Event>,
    ) {
        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
        }

        self.pending_key = Some(debounce_key.clone());
        self.pending_bucket = bucket;
        self.pending_obj_key = obj_key;

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = debounce_tx.send(Event::DebounceTimeout { debounce_key });
        });

        self.timer_handle = Some(handle);
    }
}
