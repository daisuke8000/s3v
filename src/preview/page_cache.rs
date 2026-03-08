use std::collections::HashSet;

pub const CACHE_WINDOW_RADIUS: usize = 3;

pub struct PageCacheManager {
    total_pages: usize,
}

impl PageCacheManager {
    pub fn new(total_pages: usize) -> Self {
        Self { total_pages }
    }

    /// 現在ページに対するキャッシュウィンドウのページ番号を返す
    pub fn window_pages(&self, current_page: usize) -> Vec<usize> {
        if self.total_pages == 0 {
            return Vec::new();
        }
        let start = current_page.saturating_sub(CACHE_WINDOW_RADIUS);
        let end = (current_page + CACHE_WINDOW_RADIUS).min(self.total_pages - 1);
        (start..=end).collect()
    }

    /// 現在のキャッシュ状態とページ位置から、リクエスト/破棄すべきページを返す
    pub fn update_window(
        &self,
        current_page: usize,
        cached_pages: &HashSet<usize>,
    ) -> (Vec<usize>, Vec<usize>) {
        let window: HashSet<usize> = self.window_pages(current_page).into_iter().collect();

        let mut to_request: Vec<usize> = window
            .iter()
            .filter(|p| !cached_pages.contains(p))
            .copied()
            .collect();
        to_request.sort_unstable();

        let mut to_evict: Vec<usize> = cached_pages
            .iter()
            .filter(|p| !window.contains(p))
            .copied()
            .collect();
        to_evict.sort_unstable();

        (to_request, to_evict)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_pages_middle() {
        let mgr = PageCacheManager::new(10);
        assert_eq!(mgr.window_pages(5), vec![2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn test_window_pages_start() {
        let mgr = PageCacheManager::new(10);
        assert_eq!(mgr.window_pages(0), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_window_pages_end() {
        let mgr = PageCacheManager::new(10);
        assert_eq!(mgr.window_pages(9), vec![6, 7, 8, 9]);
    }

    #[test]
    fn test_window_pages_small_pdf() {
        let mgr = PageCacheManager::new(2);
        assert_eq!(mgr.window_pages(0), vec![0, 1]);
    }

    #[test]
    fn test_window_pages_single_page() {
        let mgr = PageCacheManager::new(1);
        assert_eq!(mgr.window_pages(0), vec![0]);
    }

    #[test]
    fn test_window_pages_empty() {
        let mgr = PageCacheManager::new(0);
        assert!(mgr.window_pages(0).is_empty());
    }

    #[test]
    fn test_update_window_fresh() {
        let mgr = PageCacheManager::new(10);
        let cached = HashSet::new();
        let (to_request, to_evict) = mgr.update_window(5, &cached);
        assert!(to_evict.is_empty());
        assert_eq!(to_request, vec![2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn test_update_window_with_existing_cache() {
        let mgr = PageCacheManager::new(10);
        let cached: HashSet<usize> = [3, 4, 5, 6, 7].into_iter().collect();
        let (to_request, to_evict) = mgr.update_window(5, &cached);
        assert_eq!(to_request, vec![2, 8]);
        assert!(to_evict.is_empty());
    }

    #[test]
    fn test_update_window_eviction() {
        let mgr = PageCacheManager::new(10);
        let cached: HashSet<usize> = [0, 1, 2, 3].into_iter().collect();
        let (to_request, to_evict) = mgr.update_window(7, &cached);
        assert_eq!(to_request, vec![4, 5, 6, 7, 8, 9]);
        assert_eq!(to_evict, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_update_window_three_page_pdf() {
        let mgr = PageCacheManager::new(3);
        let cached: HashSet<usize> = [0, 1, 2].into_iter().collect();
        let (to_request, to_evict) = mgr.update_window(1, &cached);
        assert!(to_request.is_empty());
        assert!(to_evict.is_empty());
    }
}
