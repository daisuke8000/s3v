pub mod layout;

use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, Mode};
use crate::s3::S3Item;

use layout::AppLayout;

/// メイン描画関数（純粋関数）
pub fn render(app: &App, frame: &mut Frame) {
    let layout = AppLayout::new(frame.area());

    render_header(app, frame, layout.header);
    render_filter(app, frame, layout.filter);
    render_list(app, frame, layout.list);
    render_url_bar(app, frame, layout.url_bar);
    render_help(frame, layout.help);
}

fn render_header(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let path_str = format!("{}", app.current_path);
    let title = format!(
        " s3v | {} ",
        if path_str.is_empty() { "/" } else { &path_str }
    );

    let header = Paragraph::new(title)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    frame.render_widget(header, area);
}

fn render_filter(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let status = match app.mode {
        Mode::Loading => "Loading...".to_string(),
        Mode::Normal => format!("{} items", app.items.len()),
    };

    let filter = Paragraph::new(format!(" Filter: | {} ", status))
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(filter, area);
}

fn render_list(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(|item| {
            let (icon, name, size, date) = format_item(item);
            let line = Line::from(vec![
                Span::raw(format!(" {} ", icon)),
                Span::styled(
                    format!("{:<40}", name),
                    Style::default().fg(if item.is_folder() {
                        Color::Blue
                    } else {
                        Color::White
                    }),
                ),
                Span::styled(format!("{:>10}", size), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(date, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(app.cursor));

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_url_bar(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
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

    let url_bar =
        Paragraph::new(format!(" {}", url)).style(Style::default().fg(Color::Green));

    frame.render_widget(url_bar, area);
}

fn render_help(frame: &mut Frame, area: ratatui::layout::Rect) {
    let help = Paragraph::new(" [up/down or jk]Move [Enter]Open [Esc]Back [q]Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Left);

    frame.render_widget(help, area);
}

fn format_item(item: &S3Item) -> (String, String, String, String) {
    match item {
        S3Item::Bucket { name } => ("DIR".to_string(), name.clone(), String::new(), String::new()),
        S3Item::Folder { name, .. } => {
            ("DIR".to_string(), name.clone(), String::new(), String::new())
        }
        S3Item::File {
            name,
            size,
            last_modified,
            ..
        } => (
            "   ".to_string(),
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
    // AWS SDK returns ISO 8601 format, extract date part
    date.split('T').next().unwrap_or(date).to_string()
}
