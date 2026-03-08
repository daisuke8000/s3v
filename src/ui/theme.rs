use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Borders};

/// アプリケーション全体のカラーテーマ
pub struct Theme {
    pub header_fg: Color,
    pub breadcrumb_fg: Color,
    pub separator_fg: Color,
    pub folder_fg: Color,
    pub file_fg: Color,
    pub size_fg: Color,
    pub date_fg: Color,
    pub url_fg: Color,
    pub help_fg: Color,
    pub help_key_fg: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub border_fg: Color,
    pub item_count_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            header_fg: Color::Cyan,
            breadcrumb_fg: Color::White,
            separator_fg: Color::DarkGray,
            folder_fg: Color::Blue,
            file_fg: Color::White,
            size_fg: Color::DarkGray,
            date_fg: Color::DarkGray,
            url_fg: Color::Green,
            help_fg: Color::DarkGray,
            help_key_fg: Color::Cyan,
            highlight_bg: Color::Rgb(40, 40, 60),
            highlight_fg: Color::White,
            border_fg: Color::Rgb(80, 80, 120),
            item_count_fg: Color::DarkGray,
        }
    }
}

impl Theme {
    /// 標準的な角丸ボーダーブロックを生成
    pub fn block(&self) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border_fg))
    }
}

/// デフォルトテーマのシングルトン
pub fn theme() -> &'static Theme {
    use std::sync::LazyLock;
    static THEME: LazyLock<Theme> = LazyLock::new(Theme::default);
    &THEME
}
