use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// コンパクトバナーの高さ（ASCII アート 7-8行 + バージョン行 + 余白）
const BANNER_HEIGHT: u16 = 10;

/// 画面サイズに基づくレイアウト情報
#[derive(Debug, Clone)]
pub enum AppLayout {
    /// 起動スプラッシュ画面（バナーのみ全画面）
    Splash { area: Rect },
    /// バナー（上部コンパクト） + 通常操作画面
    BannerWithNormal {
        banner: Rect,
        header: Rect,
        list: Rect,
        footer: Rect,
    },
    /// バナー（上部コンパクト） + プレビュー付き画面
    BannerWithPreview {
        banner: Rect,
        header: Rect,
        list: Rect,
        preview: Rect,
        footer: Rect,
    },
}

impl AppLayout {
    /// 起動スプラッシュ用レイアウト（全画面バナー）
    pub fn splash(area: Rect) -> Self {
        Self::Splash { area }
    }

    /// バナー + 通常画面用レイアウト
    pub fn banner_with_normal(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(BANNER_HEIGHT), // Banner (compact)
                Constraint::Length(3),             // Header (border + breadcrumb)
                Constraint::Min(5),                // List (with border)
                Constraint::Length(4),             // Footer (URL + help with border)
            ])
            .split(area);

        Self::BannerWithNormal {
            banner: chunks[0],
            header: chunks[1],
            list: chunks[2],
            footer: chunks[3],
        }
    }

    /// バナー + プレビュー付き画面用レイアウト
    pub fn banner_with_preview(area: Rect) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(BANNER_HEIGHT), // Banner (compact)
                Constraint::Length(3),             // Header
                Constraint::Min(5),                // Content (list + preview)
                Constraint::Length(4),             // Footer
            ])
            .split(area);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50), // List
                Constraint::Percentage(50), // Preview
            ])
            .split(vertical[2]);

        Self::BannerWithPreview {
            banner: vertical[0],
            header: vertical[1],
            list: horizontal[0],
            preview: horizontal[1],
            footer: vertical[3],
        }
    }
}
