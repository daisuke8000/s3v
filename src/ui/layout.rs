use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// コンパクトバナーの高さ（ASCII アート 7-8行 + バージョン行 + 余白）
const BANNER_HEIGHT: u16 = 10;

/// 画面サイズに基づくレイアウト情報
#[derive(Debug, Clone)]
pub enum AppLayout {
    /// 起動スプラッシュ画面（バナーのみ全画面）
    Splash { area: Rect },
    /// バケット一覧時: 2ペイン [Current | Preview]
    TwoPane {
        banner: Rect,
        header: Rect,
        current: Rect,
        preview: Rect,
        footer: Rect,
    },
    /// バケット内: 3ペイン [Parent | Current | Preview]
    ThreePane {
        banner: Rect,
        header: Rect,
        parent: Rect,
        current: Rect,
        preview: Rect,
        footer: Rect,
    },
}

impl AppLayout {
    /// 起動スプラッシュ用レイアウト（全画面バナー）
    pub fn splash(area: Rect) -> Self {
        Self::Splash { area }
    }

    /// バケット一覧時: 2ペイン [Current | Preview]
    pub fn two_pane(area: Rect) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(BANNER_HEIGHT),
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(4),
            ])
            .split(area);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(vertical[2]);

        Self::TwoPane {
            banner: vertical[0],
            header: vertical[1],
            current: horizontal[0],
            preview: horizontal[1],
            footer: vertical[3],
        }
    }

    /// バケット内: 3ペイン [Parent | Current | Preview]
    pub fn three_pane(area: Rect) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(BANNER_HEIGHT),
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(4),
            ])
            .split(area);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 7), // Parent
                Constraint::Ratio(3, 7), // Current
                Constraint::Ratio(3, 7), // Preview
            ])
            .split(vertical[2]);

        Self::ThreePane {
            banner: vertical[0],
            header: vertical[1],
            parent: horizontal[0],
            current: horizontal[1],
            preview: horizontal[2],
            footer: vertical[3],
        }
    }
}
