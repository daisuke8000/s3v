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

use s3v::command_handler::{
    PreviewState, dispatch_event, setup_pdf_worker, start_preview_load, update_pdf_page,
};
use s3v::{App, Cli, Command, Event, S3Client};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let s3_client = S3Client::new(s3_sdk_client, region);

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

    // バナー描画（初期ロード前に1フレーム描画）
    terminal.draw(|f| s3v::ui::render(&app, f, None))?;

    // 初期ロード
    let initial_items = s3_client.list(&initial_path).await.unwrap_or_default();
    dispatch_event(&mut app, Event::ItemsLoaded(initial_items));

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
    let mut metadata_index: Option<s3v::search::MetadataIndex> = None;

    // ストリーミングイベント用チャネル
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<Event>();

    // crossterm EventStream（非同期キーイベント）
    let mut event_stream = EventStream::new();

    loop {
        // PDF ページ変更検知 → 再レンダリング
        update_pdf_page(app, &mut preview, picker).await;

        // 描画
        terminal.draw(|f| s3v::ui::render(app, f, preview.image_state.as_mut()))?;

        // 非同期イベント待機: キー入力 or ストリーミングイベント
        tokio::select! {
            Some(stream_event) = stream_rx.recv() => {
                if matches!(&stream_event, Event::PreviewImageReady) {
                    // 共有スロットからデコード済み画像を取り出し、picker で変換
                    if let Some(dyn_img) = preview.take_decoded_image() {
                        preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                    }
                }
                if matches!(&stream_event, Event::PdfDataReady) {
                    // PDF データスロットから取り出し、PdfWorker をセットアップ
                    setup_pdf_worker(app, picker, &mut preview).await;
                }
                let cmd = dispatch_event(app, stream_event);
                handle_command(app, s3_client, &mut preview, &mut metadata_index, &stream_tx, cmd).await?;
            }
            Some(Ok(crossterm_event)) = event_stream.next() => {
                if let crossterm::event::Event::Key(key) = crossterm_event {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    let event = match app.mode {
                        s3v::Mode::Filter | s3v::Mode::Preview | s3v::Mode::Search => Event::Key(key),
                        _ => Event::from_key(key),
                    };
                    let cmd = dispatch_event(app, event);

                    // Preview モードを抜けたらプレビュー状態をクリア
                    if app.mode != s3v::Mode::Preview {
                        preview.clear();
                    }

                    // Loading 状態なら即座に再描画
                    if app.mode == s3v::Mode::Loading {
                        terminal.draw(|f| s3v::ui::render(app, f, preview.image_state.as_mut()))?;
                    }

                    handle_command(app, s3_client, &mut preview, &mut metadata_index, &stream_tx, cmd).await?;
                }
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}

/// コマンド実行（副作用はここで処理）
async fn handle_command(
    app: &mut App,
    s3_client: &S3Client,
    preview: &mut PreviewState,
    metadata_index: &mut Option<s3v::search::MetadataIndex>,
    stream_tx: &mpsc::UnboundedSender<Event>,
    cmd: Option<Command>,
) -> anyhow::Result<()> {
    let Some(cmd) = cmd else {
        return Ok(());
    };

    match cmd {
        Command::Quit => {
            app.running = false;
        }
        Command::Download {
            bucket,
            key,
            destination,
        } => {
            if let Err(e) =
                s3v::download::download_file(s3_client.inner(), &bucket, &key, &destination).await
            {
                dispatch_event(app, Event::Error(format!("Download error: {}", e)));
            }
        }
        Command::LoadPreview { bucket, key } => {
            start_preview_load(app, s3_client, preview, &bucket, &key, stream_tx);
        }
        Command::LoadItems(path) => {
            match s3_client.list(&path).await {
                Ok(items) => {
                    dispatch_event(app, Event::ItemsLoaded(items));

                    // バケットに入った時にメタデータインデックスを構築
                    if let Some(bucket) = &app.current_path.bucket
                        && !app.metadata_indexed
                        && let Ok(all_items) = s3_client.list_all_objects(bucket).await
                        && let Ok(index) = s3v::search::MetadataIndex::new()
                        && let Ok(count) = index.insert_items(&all_items)
                    {
                        *metadata_index = Some(index);
                        dispatch_event(app, Event::MetadataIndexed(count));
                    }
                }
                Err(e) => {
                    dispatch_event(app, Event::Error(format!("Failed to load items: {}", e)));
                }
            }
        }
        Command::IndexMetadata { bucket } => {
            if let Ok(all_items) = s3_client.list_all_objects(&bucket).await
                && let Ok(index) = s3v::search::MetadataIndex::new()
                && let Ok(count) = index.insert_items(&all_items)
            {
                *metadata_index = Some(index);
                dispatch_event(app, Event::MetadataIndexed(count));
            }
        }
        Command::ExecuteSearch(where_clause) => {
            if let Some(index) = metadata_index.as_ref() {
                match index.search(&where_clause) {
                    Ok(results) => {
                        dispatch_event(app, Event::SearchResults(results));
                    }
                    Err(e) => {
                        dispatch_event(app, Event::Error(format!("Search error: {}", e)));
                    }
                }
            } else {
                dispatch_event(app, Event::Error("Metadata not indexed yet".to_string()));
            }
        }
    }

    Ok(())
}
