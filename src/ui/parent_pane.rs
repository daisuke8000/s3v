use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::app::App;
use crate::s3::S3Item;

use super::theme::theme;

/// 親ペイン: 親ディレクトリのアイテム一覧を描画（現在パスをハイライト）
pub fn render_parent_pane(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let items: Vec<ListItem> = app
        .parent_items
        .iter()
        .map(|item| {
            let icon = if item.is_folder() { "▸" } else { "·" };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", icon),
                    Style::default().fg(if item.is_folder() {
                        t.folder_fg
                    } else {
                        t.file_fg
                    }),
                ),
                Span::styled(
                    truncate_name(item.name(), area.width.saturating_sub(6) as usize),
                    Style::default().fg(if item.is_folder() {
                        t.folder_fg
                    } else {
                        t.file_fg
                    }),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let block = t.block();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(t.highlight_bg)
            .fg(t.highlight_fg)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(app.parent_cursor));

    frame.render_stateful_widget(list, area, &mut state);
}

/// フォルダプレビュー: カーソル上のフォルダの子アイテム一覧を描画
pub fn render_folder_preview(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let items: Vec<ListItem> = app
        .folder_preview_items
        .iter()
        .map(|item| {
            let icon = if item.is_folder() { "▸" } else { "·" };
            let name = truncate_name(item.name(), area.width.saturating_sub(6) as usize);
            let size_str = match item {
                S3Item::File { size, .. } => super::format::format_size(*size),
                _ => String::new(),
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", icon),
                    Style::default().fg(if item.is_folder() {
                        t.folder_fg
                    } else {
                        t.file_fg
                    }),
                ),
                Span::styled(
                    format!("{:<30}", name),
                    Style::default().fg(if item.is_folder() {
                        t.folder_fg
                    } else {
                        t.file_fg
                    }),
                ),
                Span::styled(format!("{:>8}", size_str), Style::default().fg(t.size_fg)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = format!(" {} items ", app.folder_preview_items.len());
    let block = t
        .block()
        .title(Span::styled(title, Style::default().fg(t.item_count_fg)));

    let list = List::new(items).block(block);

    frame.render_widget(list, area);
}

/// 名前を指定幅（文字数ベース）に切り詰める
fn truncate_name(name: &str, max_width: usize) -> String {
    if name.chars().count() <= max_width {
        name.to_string()
    } else if max_width > 3 {
        let truncated: String = name.chars().take(max_width - 3).collect();
        format!("{}...", truncated)
    } else {
        name.chars().take(max_width).collect()
    }
}
