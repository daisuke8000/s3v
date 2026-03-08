pub mod banner;
pub mod layout;
pub mod theme;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, Mode};
use crate::s3::S3Item;

use layout::AppLayout;
use theme::theme;

/// メイン描画関数（純粋関数）
pub fn render(app: &App, frame: &mut Frame) {
    if app.show_banner {
        let layout = AppLayout::banner(frame.area());
        if let AppLayout::Banner { area } = layout {
            let is_loading = app.mode == Mode::Loading;
            banner::render_banner(frame, area, is_loading);
        }
        return;
    }

    let layout = AppLayout::normal(frame.area());
    if let AppLayout::Normal {
        header,
        list,
        footer,
    } = layout
    {
        render_header(app, frame, header);
        render_list(app, frame, list);
        render_footer(app, frame, footer);
    }
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let breadcrumb = build_breadcrumb(app);

    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_fg))
        .title(Span::styled(
            " s3v ",
            Style::default()
                .fg(t.header_fg)
                .add_modifier(Modifier::BOLD),
        ));

    let header = Paragraph::new(breadcrumb).block(header_block);

    frame.render_widget(header, area);
}

fn build_breadcrumb(app: &App) -> Line<'static> {
    let t = theme();
    let sep = Span::styled(" › ", Style::default().fg(t.separator_fg));

    let mut spans = vec![Span::styled("/", Style::default().fg(t.breadcrumb_fg))];

    if let Some(bucket) = &app.current_path.bucket {
        spans.push(sep.clone());
        spans.push(Span::styled(
            bucket.clone(),
            Style::default().fg(t.breadcrumb_fg),
        ));

        if !app.current_path.prefix.is_empty() {
            for part in app.current_path.prefix.split('/').filter(|s| !s.is_empty()) {
                spans.push(sep.clone());
                spans.push(Span::styled(
                    part.to_string(),
                    Style::default().fg(t.breadcrumb_fg),
                ));
            }
            spans.push(Span::styled("/", Style::default().fg(t.separator_fg)));
        }
    }

    let item_count = format!("  {} items", app.items.len());
    spans.push(Span::styled(
        item_count,
        Style::default().fg(t.item_count_fg),
    ));

    if !app.selected.is_empty() {
        spans.push(Span::styled(
            format!("  {} selected", app.selected.len()),
            Style::default().fg(ratatui::style::Color::Green),
        ));
    }

    Line::from(spans)
}

fn render_list(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let items: Vec<ListItem> = app
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == app.cursor;
            let (icon, name, size, date) = format_item(item, is_selected);
            let selected_marker = if app.selected.contains(&i) {
                "+"
            } else {
                " "
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{}", selected_marker),
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
        Mode::Normal | Mode::Filter => String::new(),
    };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_fg))
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

fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(3)])
        .split(area);

    // URL バー
    let url = if let Some(item) = app.selected_item() {
        let path = match item {
            S3Item::Bucket { name } => crate::s3::S3Path::bucket(name),
            S3Item::Folder { prefix, .. } => crate::s3::S3Path::with_prefix(
                app.current_path.bucket.clone().unwrap_or_default(),
                prefix,
            ),
            S3Item::File { key, .. } => crate::s3::S3Path::with_prefix(
                app.current_path.bucket.clone().unwrap_or_default(),
                key,
            ),
        };
        path.to_s3_uri()
    } else {
        app.current_path.to_s3_uri()
    };

    let url_or_filter = match app.mode {
        Mode::Filter => format!(" /{}", app.filter),
        _ => format!(" {}", url),
    };
    let url_bar = Paragraph::new(url_or_filter).style(Style::default().fg(t.url_fg));
    frame.render_widget(url_bar, chunks[0]);

    // ヘルプバー
    let help = Line::from(vec![
        Span::styled(" ↑↓", Style::default().fg(t.help_key_fg)),
        Span::styled("/", Style::default().fg(t.separator_fg)),
        Span::styled("jk", Style::default().fg(t.help_key_fg)),
        Span::styled(" Move  ", Style::default().fg(t.help_fg)),
        Span::styled("⏎", Style::default().fg(t.help_key_fg)),
        Span::styled(" Open  ", Style::default().fg(t.help_fg)),
        Span::styled("⎋", Style::default().fg(t.help_key_fg)),
        Span::styled(" Back  ", Style::default().fg(t.help_fg)),
        Span::styled("q", Style::default().fg(t.help_key_fg)),
        Span::styled(" Quit", Style::default().fg(t.help_fg)),
    ]);

    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_fg));

    let help_bar = Paragraph::new(help)
        .block(help_block)
        .alignment(Alignment::Center);
    frame.render_widget(help_bar, chunks[1]);
}

fn format_item(item: &S3Item, is_selected: bool) -> (String, String, String, String) {
    let folder_icon = if is_selected { "▶" } else { "▸" };
    let file_icon = if is_selected { "▶" } else { "·" };

    match item {
        S3Item::Bucket { name } => (
            folder_icon.to_string(),
            name.clone(),
            String::new(),
            String::new(),
        ),
        S3Item::Folder { name, .. } => (
            folder_icon.to_string(),
            name.clone(),
            String::new(),
            String::new(),
        ),
        S3Item::File {
            name,
            size,
            last_modified,
            ..
        } => (
            file_icon.to_string(),
            name.clone(),
            format_size(*size),
            last_modified
                .as_ref()
                .map(|d| format_date(d))
                .unwrap_or_default(),
        ),
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_date(date: &str) -> String {
    date.split('T').next().unwrap_or(date).to_string()
}
