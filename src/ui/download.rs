use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
};

use crate::app::App;
use crate::app::download::{ConfirmButton, ConfirmFocus, DownloadTarget};

use super::format::format_size;

/// 確認ダイアログの描画（画面中央オーバーレイ）
pub fn render_confirm_dialog(app: &App, frame: &mut Frame) {
    let area = centered_rect(50, 50, frame.area());

    // 背景クリア
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Download Confirm ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // target info
            Constraint::Length(1), // spacing
            Constraint::Length(1), // save path label
            Constraint::Length(1), // save path input
            Constraint::Length(1), // completions
            Constraint::Length(1), // spacing
            Constraint::Length(1), // buttons
        ])
        .split(inner);

    // ダウンロード対象情報
    let target_lines = match &app.download_target {
        Some(DownloadTarget::SingleFile { name, size, .. }) => {
            vec![
                Line::from(Span::styled(
                    format!("  {} ", name),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("  {}", format_size(*size)),
                    Style::default().fg(Color::Gray),
                )),
            ]
        }
        Some(DownloadTarget::Folder {
            name,
            file_count,
            total_size,
            ..
        }) => {
            vec![
                Line::from(Span::styled(
                    format!("  {} ", name),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("  {} files  ({})", file_count, format_size(*total_size)),
                    Style::default().fg(Color::Gray),
                )),
            ]
        }
        None => vec![],
    };
    let target_paragraph = Paragraph::new(target_lines);
    frame.render_widget(target_paragraph, chunks[0]);

    // 保存先ラベル
    let path_label = Paragraph::new(Span::styled("  Save to:", Style::default().fg(Color::Gray)));
    frame.render_widget(path_label, chunks[2]);

    // 保存先入力
    let path_style = if app.confirm_focus == ConfirmFocus::Path {
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let cursor_char = if app.confirm_focus == ConfirmFocus::Path {
        "_"
    } else {
        ""
    };
    let path_text = format!("  {}{}", app.download_path, cursor_char);
    let path_input = Paragraph::new(Span::styled(path_text, path_style));
    frame.render_widget(path_input, chunks[3]);

    // Tab 補完候補表示
    if app.confirm_focus == ConfirmFocus::Path && !app.path_completions.is_empty() {
        let comp_text = app
            .path_completions
            .iter()
            .take(3)
            .map(|c| c.as_str())
            .collect::<Vec<&str>>()
            .join("  ");
        let comp = Paragraph::new(Span::styled(
            format!("  Tab: {}", comp_text),
            Style::default().fg(Color::DarkGray),
        ));
        frame.render_widget(comp, chunks[4]);
    }

    // ボタン行
    let start_style = button_style(
        app.confirm_focus == ConfirmFocus::Buttons && app.confirm_button == ConfirmButton::Start,
    );
    let cancel_style = button_style(
        app.confirm_focus == ConfirmFocus::Buttons && app.confirm_button == ConfirmButton::Cancel,
    );

    let buttons = Line::from(vec![
        Span::raw("      "),
        Span::styled(" Start ", start_style),
        Span::raw("    "),
        Span::styled(" Cancel ", cancel_style),
    ]);
    let btn_paragraph = Paragraph::new(buttons);
    frame.render_widget(btn_paragraph, chunks[6]);
}

/// 進捗ダイアログの描画（画面中央オーバーレイ）
pub fn render_progress_dialog(app: &App, frame: &mut Frame) {
    let area = centered_rect(50, 25, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Download ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // current file
            Constraint::Length(1), // spacing
            Constraint::Length(1), // gauge
            Constraint::Length(1), // spacing
            Constraint::Length(1), // esc hint
        ])
        .split(inner);

    if let Some(progress) = &app.download_progress {
        // 現在のファイル名
        let file_text = if progress.current_file.is_empty() {
            "  Preparing...".to_string()
        } else {
            format!("  {}", progress.current_file)
        };
        let file_line = Paragraph::new(Span::styled(file_text, Style::default().fg(Color::White)));
        frame.render_widget(file_line, chunks[0]);

        // プログレスバー
        let ratio = if progress.total > 0 {
            progress.completed as f64 / progress.total as f64
        } else {
            0.0
        };
        let label = format!(
            "{}/{} ({}%)",
            progress.completed,
            progress.total,
            (ratio * 100.0) as u16
        );
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
            .ratio(ratio)
            .label(label);
        frame.render_widget(gauge, chunks[2]);
    }

    // Esc ヒント
    let hint = Paragraph::new(Span::styled(
        "  Esc to cancel",
        Style::default().fg(Color::DarkGray),
    ))
    .alignment(Alignment::Left);
    frame.render_widget(hint, chunks[4]);
}

/// ボタンのスタイル
fn button_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray).bg(Color::DarkGray)
    }
}

/// 画面中央に percent_x% x percent_y% の矩形を返す
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
