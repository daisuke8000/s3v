use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// 画面サイズに基づくレイアウト情報
#[derive(Debug, Clone)]
pub enum AppLayout {
    /// 起動バナー画面
    Banner { area: Rect },
    /// 通常操作画面
    Normal {
        header: Rect,
        list: Rect,
        footer: Rect,
    },
    /// プレビュー付き画面（リスト左 + プレビュー右）
    WithPreview {
        header: Rect,
        list: Rect,
        preview: Rect,
        footer: Rect,
    },
}

impl AppLayout {
    /// 起動バナー用レイアウト
    pub fn banner(area: Rect) -> Self {
        Self::Banner { area }
    }

    /// 通常画面用レイアウト
    pub fn normal(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header (border + breadcrumb)
                Constraint::Min(5),    // List (with border)
                Constraint::Length(4), // Footer (URL + help with border)
            ])
            .split(area);

        Self::Normal {
            header: chunks[0],
            list: chunks[1],
            footer: chunks[2],
        }
    }

    /// プレビュー付き画面用レイアウト（リスト左 + プレビュー右）
    pub fn with_preview(area: Rect) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(5),    // Content (list + preview)
                Constraint::Length(4), // Footer
            ])
            .split(area);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50), // List
                Constraint::Percentage(50), // Preview
            ])
            .split(vertical[1]);

        Self::WithPreview {
            header: vertical[0],
            list: horizontal[0],
            preview: horizontal[1],
            footer: vertical[2],
        }
    }
}
