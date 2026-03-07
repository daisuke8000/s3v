use ansi_to_tui::IntoText;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
};

/// 起動バナーを描画する
pub fn render_banner(frame: &mut Frame, area: Rect) {
    let banner_text = generate_banner();
    let version_line = Line::from(vec![
        Span::styled(
            "S3 Viewer TUI",
            Style::default()
                .fg(Color::Rgb(100, 200, 255))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let loading_line = Line::from(Span::styled(
        "Loading...",
        Style::default().fg(Color::DarkGray),
    ));

    let mut lines = banner_text.lines;
    lines.push(Line::raw(""));
    lines.push(version_line);
    lines.push(loading_line);

    let banner = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);

    frame.render_widget(banner, area);
}

/// tui-banner で ASCII アートを生成し、ansi-to-tui で ratatui Text に変換
fn generate_banner() -> Text<'static> {
    let ansi_string = tui_banner::Banner::new("s3v")
        .ok()
        .map(|b| b.style(tui_banner::Style::NeonCyber).render())
        .unwrap_or_else(|| "s3v".to_string());

    ansi_string.into_text().unwrap_or_else(|_| Text::raw("s3v"))
}
