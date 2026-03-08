use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{Paragraph, Wrap},
};
use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;

use crate::app::App;
use crate::preview::PreviewContent;

use super::theme::theme;

pub fn render_preview(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    image_state: Option<&mut StatefulProtocol>,
) {
    let t = theme();
    let block = t.block().title(Span::styled(
        " Preview ",
        Style::default()
            .fg(t.header_fg)
            .add_modifier(Modifier::BOLD),
    ));

    match &app.preview_content {
        Some(PreviewContent::Text(content)) => {
            let paragraph = Paragraph::new(content.as_str())
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((app.preview_scroll, 0));
            frame.render_widget(paragraph, area);
        }
        Some(PreviewContent::Image(_)) => {
            if let Some(state) = image_state {
                let inner = block.inner(area);
                frame.render_widget(block, area);
                let image_widget = StatefulImage::default();
                frame.render_stateful_widget(image_widget, inner, state);
            }
        }
        Some(PreviewContent::Pdf {
            current_page,
            total_pages,
        }) => {
            let title = format!(" PDF [{}/{}] ", current_page + 1, total_pages);
            let block = block.title(Span::styled(
                title,
                Style::default()
                    .fg(t.header_fg)
                    .add_modifier(Modifier::BOLD),
            ));
            if let Some(state) = image_state {
                let inner = block.inner(area);
                frame.render_widget(block, area);
                let image_widget = StatefulImage::default();
                frame.render_stateful_widget(image_widget, inner, state);
            }
        }
        _ => {}
    }
}
