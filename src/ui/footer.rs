use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, Mode};

use super::theme::theme;

pub fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(3)])
        .split(area);

    // URL バー
    let url = if let Some(item) = app.selected_item() {
        app.item_to_s3_path(item).to_s3_uri()
    } else {
        app.current_path.to_s3_uri()
    };

    let (url_or_filter, url_color) = if let Some(ref err) = app.error_message {
        (format!(" Error: {}", err), ratatui::style::Color::Red)
    } else if let Some(ref msg) = app.status_message {
        (format!(" {}", msg), ratatui::style::Color::Green)
    } else {
        let text = match app.mode {
            Mode::Filter => format!(" /{}", app.filter),
            Mode::Search => {
                if app.indexing_in_progress {
                    format!(
                        " SQL> {}  [Indexing... {}]",
                        app.search_query, app.indexing_count
                    )
                } else {
                    format!(" SQL> {}", app.search_query)
                }
            }
            _ => {
                if app.has_more {
                    format!(" {} ({} items, more available)", url, app.items.len())
                } else if !app.items.is_empty() {
                    format!(" {} ({} items)", url, app.items.len())
                } else {
                    format!(" {}", url)
                }
            }
        };
        (text, t.url_fg)
    };
    let url_bar = Paragraph::new(url_or_filter).style(Style::default().fg(url_color));
    frame.render_widget(url_bar, chunks[0]);

    // ヘルプバー（モード別）
    let help = build_help_line(app);

    let help_block = t.block();

    let help_bar = Paragraph::new(help)
        .block(help_block)
        .alignment(Alignment::Center);
    frame.render_widget(help_bar, chunks[1]);
}

fn build_help_line(app: &App) -> Line<'static> {
    let t = theme();

    match app.mode {
        Mode::Normal => Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("jk", Style::default().fg(t.help_key_fg)),
            Span::styled(" Move  ", Style::default().fg(t.help_fg)),
            Span::styled("←→", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("hl", Style::default().fg(t.help_key_fg)),
            Span::styled(" Nav  ", Style::default().fg(t.help_fg)),
            Span::styled("⏎", Style::default().fg(t.help_key_fg)),
            Span::styled(" Open  ", Style::default().fg(t.help_fg)),
            Span::styled("Space", Style::default().fg(t.help_key_fg)),
            Span::styled(" Sel  ", Style::default().fg(t.help_fg)),
            Span::styled("a", Style::default().fg(t.help_key_fg)),
            Span::styled(" All  ", Style::default().fg(t.help_fg)),
            Span::styled("d", Style::default().fg(t.help_key_fg)),
            Span::styled(" DL  ", Style::default().fg(t.help_fg)),
            Span::styled("y", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("Y", Style::default().fg(t.help_key_fg)),
            Span::styled(" Copy  ", Style::default().fg(t.help_fg)),
            Span::styled("/", Style::default().fg(t.help_key_fg)),
            Span::styled(" Filter  ", Style::default().fg(t.help_fg)),
            Span::styled("?", Style::default().fg(t.help_key_fg)),
            Span::styled(" SQL  ", Style::default().fg(t.help_fg)),
            Span::styled("Tab", Style::default().fg(t.help_key_fg)),
            Span::styled(" Focus  ", Style::default().fg(t.help_fg)),
            Span::styled("q", Style::default().fg(t.help_key_fg)),
            Span::styled(" Quit", Style::default().fg(t.help_fg)),
        ]),
        Mode::Filter => Line::from(vec![
            Span::styled(" ⏎", Style::default().fg(t.help_key_fg)),
            Span::styled(" Apply  ", Style::default().fg(t.help_fg)),
            Span::styled("⎋", Style::default().fg(t.help_key_fg)),
            Span::styled(" Cancel  ", Style::default().fg(t.help_fg)),
            Span::styled("Type to filter", Style::default().fg(t.help_fg)),
        ]),
        Mode::PreviewFocus => Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("jk", Style::default().fg(t.help_key_fg)),
            Span::styled(" Scroll  ", Style::default().fg(t.help_fg)),
            Span::styled("←→", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("hl", Style::default().fg(t.help_key_fg)),
            Span::styled(" Page  ", Style::default().fg(t.help_fg)),
            Span::styled("Tab", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("⎋", Style::default().fg(t.help_key_fg)),
            Span::styled(" Back", Style::default().fg(t.help_fg)),
        ]),
        Mode::Search => Line::from(vec![
            Span::styled(" ⏎", Style::default().fg(t.help_key_fg)),
            Span::styled(" Execute  ", Style::default().fg(t.help_fg)),
            Span::styled("⎋", Style::default().fg(t.help_key_fg)),
            Span::styled(" Cancel  ", Style::default().fg(t.help_fg)),
            Span::styled("Type SQL WHERE clause", Style::default().fg(t.help_fg)),
        ]),
        Mode::Loading => Line::from(vec![Span::styled(
            " Loading...",
            Style::default().fg(t.help_fg),
        )]),
        Mode::DownloadConfirm => Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(t.help_key_fg)),
            Span::styled(" Focus  ", Style::default().fg(t.help_fg)),
            Span::styled("←→", Style::default().fg(t.help_key_fg)),
            Span::styled(" Button  ", Style::default().fg(t.help_fg)),
            Span::styled("⏎", Style::default().fg(t.help_key_fg)),
            Span::styled(" Select  ", Style::default().fg(t.help_fg)),
            Span::styled("Tab", Style::default().fg(t.help_key_fg)),
            Span::styled(" Complete  ", Style::default().fg(t.help_fg)),
            Span::styled("⎋", Style::default().fg(t.help_key_fg)),
            Span::styled(" Cancel", Style::default().fg(t.help_fg)),
        ]),
        Mode::Downloading => Line::from(vec![
            Span::styled(" Downloading...  ", Style::default().fg(t.help_fg)),
            Span::styled("⎋", Style::default().fg(t.help_key_fg)),
            Span::styled(" Cancel", Style::default().fg(t.help_fg)),
        ]),
    }
}
