use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Gauge, Paragraph, Wrap},
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
        Some(PreviewContent::Image) => {
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
        Some(PreviewContent::StreamingText { partial_text, .. }) => {
            let block = block.title(Span::styled(
                " (streaming...) ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ));
            let paragraph = Paragraph::new(partial_text.as_str())
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((app.preview_scroll, 0));
            frame.render_widget(paragraph, area);
        }
        Some(PreviewContent::Downloading { received, total }) => {
            let inner = block.inner(area);
            frame.render_widget(block, area);

            let (ratio, label) = match total {
                Some(total) if *total > 0 => {
                    let r = *received as f64 / *total as f64;
                    let label = format!(
                        "{} / {} ({:.0}%)",
                        format_bytes(*received),
                        format_bytes(*total),
                        r * 100.0,
                    );
                    (r, label)
                }
                _ => {
                    let label = format!("{} downloaded", format_bytes(*received));
                    (0.0, label)
                }
            };

            // Gauge を inner の垂直中央に1行だけ配置
            let gauge_y = inner.y + inner.height / 2;
            let gauge_area = Rect::new(inner.x + 2, gauge_y, inner.width.saturating_sub(4), 1);

            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                .ratio(ratio.clamp(0.0, 1.0))
                .label(label);
            frame.render_widget(gauge, gauge_area);
        }
        None => {}
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
