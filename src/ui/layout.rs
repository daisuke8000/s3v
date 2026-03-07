use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// 画面サイズに基づくレイアウト情報
#[derive(Debug, Clone)]
pub struct AppLayout {
    pub header: Rect,
    pub filter: Rect,
    pub list: Rect,
    pub url_bar: Rect,
    pub help: Rect,
}

impl AppLayout {
    pub fn new(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Header
                Constraint::Length(1), // Filter bar
                Constraint::Min(5),    // File list
                Constraint::Length(2), // URL bar
                Constraint::Length(1), // Help
            ])
            .split(area);

        Self {
            header: chunks[0],
            filter: chunks[1],
            list: chunks[2],
            url_bar: chunks[3],
            help: chunks[4],
        }
    }
}
