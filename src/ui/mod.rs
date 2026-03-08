pub mod banner;
pub mod layout;
pub mod preview;
pub mod theme;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};
use ratatui_image::protocol::StatefulProtocol;

use crate::app::{App, BannerState, Mode};
use crate::s3::S3Item;

use layout::AppLayout;
use theme::theme;

/// メイン描画関数（純粋関数）
pub fn render(app: &App, frame: &mut Frame, image_state: Option<&mut StatefulProtocol>) {
    match app.banner_state {
        BannerState::Splash => {
            let layout = AppLayout::splash(frame.area());
            if let AppLayout::Splash { area } = layout {
                let is_loading = app.mode == Mode::Loading;
                banner::render_banner(frame, area, is_loading);
            }
        }
        BannerState::Active => {
            if app.mode == Mode::Preview {
                let layout = AppLayout::banner_with_preview(frame.area());
                if let AppLayout::BannerWithPreview {
                    banner: banner_area,
                    header,
                    list,
                    preview,
                    footer,
                } = layout
                {
                    banner::render_compact_banner(frame, banner_area);
                    render_header(app, frame, header);
                    render_list(app, frame, list);
                    preview::render_preview(app, frame, preview, image_state);
                    render_footer(app, frame, footer);
                }
            } else {
                let layout = AppLayout::banner_with_normal(frame.area());
                if let AppLayout::BannerWithNormal {
                    banner: banner_area,
                    header,
                    list,
                    footer,
                } = layout
                {
                    banner::render_compact_banner(frame, banner_area);
                    render_header(app, frame, header);
                    render_list(app, frame, list);
                    render_footer(app, frame, footer);
                }
            }
        }
    }
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
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

fn render_list(app: &App, frame: &mut Frame, area: Rect) {
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

    let url_or_filter = if let Some(ref err) = app.error_message {
        format!(" Error: {}", err)
    } else {
        match app.mode {
            Mode::Filter => format!(" /{}", app.filter),
            Mode::Search => format!(" SQL> {}", app.search_query),
            _ => format!(" {}", url),
        }
    };
    let url_color = if app.error_message.is_some() {
        ratatui::style::Color::Red
    } else {
        t.url_fg
    };
    let url_bar = Paragraph::new(url_or_filter).style(Style::default().fg(url_color));
    frame.render_widget(url_bar, chunks[0]);

    // ヘルプバー（モード別）
    let help = match app.mode {
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
            Span::styled("/", Style::default().fg(t.help_key_fg)),
            Span::styled(" Filter  ", Style::default().fg(t.help_fg)),
            Span::styled("?", Style::default().fg(t.help_key_fg)),
            Span::styled(" SQL  ", Style::default().fg(t.help_fg)),
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
        Mode::Preview => Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("jk", Style::default().fg(t.help_key_fg)),
            Span::styled(" Scroll  ", Style::default().fg(t.help_fg)),
            Span::styled("←→", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("hl", Style::default().fg(t.help_key_fg)),
            Span::styled(" Page  ", Style::default().fg(t.help_fg)),
            Span::styled("⎋", Style::default().fg(t.help_key_fg)),
            Span::styled("/", Style::default().fg(t.separator_fg)),
            Span::styled("q", Style::default().fg(t.help_key_fg)),
            Span::styled(" Close", Style::default().fg(t.help_fg)),
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
    };

    let help_block = t.block();

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
