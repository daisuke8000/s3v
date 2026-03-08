use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::App;

use super::theme::theme;

pub fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let breadcrumb = build_breadcrumb(app);

    let header_block = t.block().title(Span::styled(
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

    if app.metadata_indexed {
        spans.push(Span::styled(
            format!("  [{} indexed]", app.metadata_count),
            Style::default().fg(t.size_fg),
        ));
    }

    Line::from(spans)
}
