use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use crate::app::App;
use crate::preview::PreviewContent;

use super::theme::theme;

pub fn render_preview(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_fg))
        .title(Span::styled(
            " Preview ",
            Style::default()
                .fg(t.header_fg)
                .add_modifier(Modifier::BOLD),
        ));

    if let Some(PreviewContent::Text(content)) = &app.preview_content {
        let paragraph = Paragraph::new(content.as_str())
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.preview_scroll, 0));
        frame.render_widget(paragraph, area);
    }
}
