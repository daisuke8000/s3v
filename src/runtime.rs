use std::collections::HashMap;
use std::time::{Duration, Instant};

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
