use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::app::{App, Mode};
use crate::s3::S3Item;

use super::format::{format_date, format_size};
use super::theme::theme;

pub fn render_list(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let items: Vec<ListItem> = app
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == app.cursor;
            let (icon, name, size, date) = format_item(item, is_selected);
            let selected_marker = if app.selected.contains(&i) { "+" } else { " " };
            let line = Line::from(vec![
                Span::styled(
                    selected_marker.to_string(),
                    Style::default().fg(ratatui::style::Color::Green),
                ),
                Span::styled(
                    format!(" {} ", icon),
                    Style::default().fg(if item.is_folder() {
                        t.folder_fg
                    } else {
                        t.file_fg
                    }),
                ),
                Span::styled(
                    format!("{:<40}", name),
                    Style::default().fg(if item.is_folder() {
                        t.folder_fg
                    } else {
                        t.file_fg
                    }),
                ),
                Span::styled(format!("{:>10}", size), Style::default().fg(t.size_fg)),
                Span::raw("  "),
                Span::styled(date, Style::default().fg(t.date_fg)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let status = match app.mode {
        Mode::Loading => " Loading... ".to_string(),
        _ => String::new(),
    };

    let list_block = t
        .block()
        .title(Span::styled(status, Style::default().fg(t.header_fg)));

    let list = List::new(items).block(list_block).highlight_style(
        Style::default()
            .bg(t.highlight_bg)
            .fg(t.highlight_fg)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(app.cursor));

    frame.render_stateful_widget(list, area, &mut state);
}

fn format_item(item: &S3Item, is_selected: bool) -> (&str, &str, String, String) {
    let folder_icon = if is_selected { "▶" } else { "▸" };
    let file_icon = if is_selected { "▶" } else { "·" };

    match item {
        S3Item::Bucket { name } => (folder_icon, name, String::new(), String::new()),
        S3Item::Folder { name, .. } => (folder_icon, name, String::new(), String::new()),
        S3Item::File {
            name,
            size,
            last_modified,
            ..
        } => (
            file_icon,
            name,
            format_size(*size),
            last_modified
                .as_ref()
                .map(|d| format_date(d))
                .unwrap_or_default(),
        ),
    }
}
