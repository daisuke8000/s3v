pub mod banner;
pub mod download;
pub mod footer;
pub mod format;
pub mod header;
pub mod layout;
pub mod list;
pub mod parent_pane;
pub mod preview;
pub mod theme;

use ratatui::Frame;
use ratatui_image::protocol::StatefulProtocol;

use crate::app::{App, BannerState, Mode};

use layout::AppLayout;

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
            if app.current_path.is_root() {
                // バケット一覧: 2ペイン [Current | Preview]
                render_two_pane(app, frame, image_state);
            } else {
                // バケット内: 3ペイン [Parent | Current | Preview]
                render_three_pane(app, frame, image_state);
            }

            // ダウンロードオーバーレイ
            match app.mode {
                Mode::DownloadConfirm => download::render_confirm_dialog(app, frame),
                Mode::Downloading => download::render_progress_dialog(app, frame),
                _ => {}
            }
        }
    }
}

/// バケット一覧: 2ペイン [Current | Preview]
fn render_two_pane(app: &App, frame: &mut Frame, image_state: Option<&mut StatefulProtocol>) {
    let layout = AppLayout::two_pane(frame.area());
    if let AppLayout::TwoPane {
        banner: banner_area,
        header: header_area,
        current,
        preview: preview_area,
        footer: footer_area,
    } = layout
    {
        banner::render_compact_banner(frame, banner_area);
        header::render_header(app, frame, header_area);
        list::render_list(app, frame, current);
        render_preview_pane(app, frame, preview_area, image_state);
        footer::render_footer(app, frame, footer_area);
    }
}

/// バケット内: 3ペイン [Parent | Current | Preview]
fn render_three_pane(app: &App, frame: &mut Frame, image_state: Option<&mut StatefulProtocol>) {
    let layout = AppLayout::three_pane(frame.area());
    if let AppLayout::ThreePane {
        banner: banner_area,
        header: header_area,
        parent,
        current,
        preview: preview_area,
        footer: footer_area,
    } = layout
    {
        banner::render_compact_banner(frame, banner_area);
        header::render_header(app, frame, header_area);
        parent_pane::render_parent_pane(app, frame, parent);
        list::render_list(app, frame, current);
        render_preview_pane(app, frame, preview_area, image_state);
        footer::render_footer(app, frame, footer_area);
    }
}

/// プレビューペインの描画（フォルダ/ファイル分岐）
fn render_preview_pane(
    app: &App,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    image_state: Option<&mut StatefulProtocol>,
) {
    // カーソル位置のアイテムがフォルダで、フォルダプレビューがある場合
    let is_folder_cursor = app.selected_item().is_some_and(|item| item.is_folder());

    if is_folder_cursor && !app.folder_preview_items.is_empty() {
        parent_pane::render_folder_preview(app, frame, area);
    } else if app.preview_content.is_some() {
        preview::render_preview(app, frame, area, image_state);
    } else {
        // 空のプレビューペイン
        let t = theme::theme();
        let block = t.block().title(ratatui::text::Span::styled(
            " Preview ",
            ratatui::style::Style::default()
                .fg(t.header_fg)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
        frame.render_widget(block, area);
    }
}
