use std::io;

use clap::Parser;
use crossterm::{
    event::{EventStream, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use ratatui_image::picker::Picker;
use tokio::sync::mpsc;

use s3v::command_executor::{CommandContext, handle_commands};
use s3v::command_handler::{
    PreviewState, dispatch_event, setup_pdf_worker, start_debounced_preview, update_pdf_page,
};
use s3v::runtime::DebounceState;
use s3v::{App, Cli, Event, S3Client};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    s3v::logging::init_logging()?;

    let cli = Cli::parse();

    // AWS SDK 設定
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

    if let Some(profile) = &cli.profile {
        config_loader = config_loader.profile_name(profile);
    }

    if let Some(region) = &cli.region {
        config_loader = config_loader.region(aws_config::Region::new(region.clone()));
    }

    let config = config_loader.load().await;
    let region = config
        .region()
        .map(|r| r.to_string())
        .unwrap_or_else(|| "us-east-1".to_string());

    let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&config);

    if let Some(endpoint) = &cli.endpoint {
        s3_config_builder = s3_config_builder
            .endpoint_url(endpoint)
            .force_path_style(true);
    }

    let s3_config = s3_config_builder.build();
    let s3_sdk_client = aws_sdk_s3::Client::from_conf(s3_config);
    let s3_client = S3Client::new(s3_sdk_client, region.clone());

    // Terminal 初期化
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 画像プレビュー用 Picker 初期化
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    // アプリケーション初期化
    let initial_path = cli.initial_path();
    let mut app = App::new();
    app.current_path = initial_path.clone();
    app.region = region.clone();

    // バナー描画（初期ロード前に1フレーム描画）
    terminal.draw(|f| s3v::ui::render(&app, f, None))?;

    // 初期ロード
    let initial_result = s3_client.list(&initial_path).await;
    match initial_result {
        Ok(result) => {
            dispatch_event(
                &mut app,
                Event::ItemsLoaded {
                    items: result.items,
                    next_token: result.next_token,
                },
            );
        }
        Err(_) => {
            dispatch_event(
                &mut app,
                Event::ItemsLoaded {
                    items: Vec::new(),
                    next_token: None,
                },
            );
        }
    }

    // メインループ
    let result = run_app(&mut terminal, &mut app, &s3_client, &mut picker).await;

    // Terminal クリーンアップ
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    s3_client: &S3Client,
    picker: &mut Picker,
) -> anyhow::Result<()> {
    let mut preview = PreviewState::new();
    let mut ctx = CommandContext {
        metadata_index: None,
        indexing_prefix: None,
        debounce: DebounceState::new(),
        runtime: s3v::runtime::RuntimeState::new(),
    };

    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<Event>();
    let mut event_stream = EventStream::new();

    loop {
        // メッセージの自動消去チェック（3秒後）
        if ctx.runtime.should_dismiss_error() {
            app.error_message = None;
        }
        if ctx.runtime.should_dismiss_status() {
            app.status_message = None;
        }

        // PDF ページ変更検知（PDF 表示中のみ）
        if matches!(
            app.preview_content,
            Some(s3v::preview::PreviewContent::Pdf { .. })
        ) {
            update_pdf_page(app, &mut preview, picker).await;
        }

        // 描画
        terminal.draw(|f| s3v::ui::render(app, f, preview.image_state.as_mut()))?;

        // 非同期イベント待機
        tokio::select! {
            Some(stream_event) = stream_rx.recv() => {
                if matches!(&stream_event, Event::PreviewImageReady)
                    && let Some(dyn_img) = preview.take_decoded_image()
                {
                    preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                }
                if matches!(&stream_event, Event::PdfDataReady) {
                    setup_pdf_worker(app, picker, &mut preview).await;
                }
                if let Event::DebounceTimeout { ref debounce_key } = stream_event
                    && ctx.debounce.pending_key.as_deref() == Some(debounce_key)
                {
                    let bucket = ctx.debounce.pending_bucket.clone();
                    let obj_key = ctx.debounce.pending_obj_key.clone();
                    let dk = debounce_key.clone();
                    start_debounced_preview(
                        app, s3_client, &mut preview,
                        &bucket, &obj_key, &dk, &stream_tx,
                    );
                }
                if matches!(
                    &stream_event,
                    Event::MetadataIndexed(_) | Event::Error(_)
                ) {
                    ctx.indexing_prefix = None;
                }
                let cmds = dispatch_event(app, stream_event);
                handle_commands(app, s3_client, &mut preview, &mut ctx, &stream_tx, cmds).await?;
            }
            Some(Ok(crossterm_event)) = event_stream.next() => {
                if let crossterm::event::Event::Key(key) = crossterm_event {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    let event = if app.mode.requires_raw_key_input() {
                        Event::Key(key)
                    } else {
                        Event::from_key(key)
                    };
                    let cmds = dispatch_event(app, event);

                    if !app.mode.preserves_preview() {
                        preview.clear();
                    }

                    if app.mode == s3v::Mode::Loading {
                        terminal.draw(|f| s3v::ui::render(app, f, preview.image_state.as_mut()))?;
                    }

                    handle_commands(app, s3_client, &mut preview, &mut ctx, &stream_tx, cmds).await?;
                }
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
