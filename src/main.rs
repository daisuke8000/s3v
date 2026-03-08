use std::io;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

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
    let (new_app, _) = app.handle_event(Event::ItemsLoaded(initial_items));
    app = new_app;

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
    let mut image_state: Option<StatefulProtocol> = None;
    let mut pdf_raw_bytes: Option<Vec<u8>> = None;
    let mut last_pdf_page: Option<usize> = None;
    let mut metadata_index: Option<s3v::search::MetadataIndex> = None;

    loop {
        // 描画
        terminal.draw(|f| s3v::ui::render(app, f, image_state.as_mut()))?;

        // イベント待機
        if event::poll(Duration::from_millis(100))?
            && let CrosstermEvent::Key(key) = event::read()?
        {
            // KeyPress のみ処理（KeyRelease は無視）
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let event = match app.mode {
                s3v::Mode::Filter | s3v::Mode::Preview | s3v::Mode::Search => Event::Key(key),
                _ => Event::from_key(key),
            };
            let (new_app, cmd) = std::mem::take(app).handle_event(event);
            *app = new_app;

            // Preview モードを抜けたら image_state をクリア
            if app.mode != s3v::Mode::Preview {
                image_state = None;
                pdf_raw_bytes = None;
                last_pdf_page = None;
            }

            // PDF ページ送り検知: current_page が変化したら再レンダリング
            if let Some(s3v::preview::PreviewContent::Pdf { current_page, .. }) =
                &app.preview_content
                && last_pdf_page != Some(*current_page)
            {
                if let Some(ref pdf_bytes) = pdf_raw_bytes
                    && let Ok(page_png) = s3v::preview::pdf::render_page(pdf_bytes, *current_page)
                    && let Ok(dyn_img) = image::load_from_memory(&page_png)
                {
                    image_state = Some(picker.new_resize_protocol(dyn_img));
                }
                last_pdf_page = Some(*current_page);
            }

            // コマンド実行（副作用はここで処理）
            if let Some(cmd) = cmd {
                match cmd {
                    Command::Quit => break,
                    Command::Download {
                        bucket,
                        key,
                        destination,
                    } => {
                        if let Err(e) = s3v::download::download_file(
                            s3_client.inner(),
                            &bucket,
                            &key,
                            &destination,
                        )
                        .await
                        {
                            let (new_app, _) = std::mem::take(app)
                                .handle_event(Event::Error(format!("Download error: {}", e)));
                            *app = new_app;
                        }
                    }
                    Command::LoadPreview { bucket, key } => {
                        handle_load_preview(
                            app,
                            s3_client,
                            picker,
                            &mut image_state,
                            &mut pdf_raw_bytes,
                            &bucket,
                            &key,
                        )
                        .await;
                        // PDF ページ追跡の初期化
                        if let Some(s3v::preview::PreviewContent::Pdf { current_page, .. }) =
                            &app.preview_content
                        {
                            last_pdf_page = Some(*current_page);
                        }
                    }
                    Command::LoadItems(path) => {
                        let items = s3_client.list(&path).await.unwrap_or_else(|e| {
                            eprintln!("Error loading items: {}", e);
                            Vec::new()
                        });
                        let (new_app, _) =
                            std::mem::take(app).handle_event(Event::ItemsLoaded(items));
                        *app = new_app;

                        // バケットに入った時にメタデータインデックスを構築
                        if let Some(bucket) = &app.current_path.bucket
                            && !app.metadata_indexed
                            && let Ok(all_items) = s3_client.list_all_objects(bucket).await
                            && let Ok(index) = s3v::search::MetadataIndex::new()
                            && let Ok(count) = index.insert_items(&all_items)
                        {
                            metadata_index = Some(index);
                            let (new_app, _) =
                                std::mem::take(app).handle_event(Event::MetadataIndexed(count));
                            *app = new_app;
                        }
                    }
                    Command::IndexMetadata { bucket } => {
                        if let Ok(all_items) = s3_client.list_all_objects(&bucket).await
                            && let Ok(index) = s3v::search::MetadataIndex::new()
                            && let Ok(count) = index.insert_items(&all_items)
                        {
                            metadata_index = Some(index);
                            let (new_app, _) =
                                std::mem::take(app).handle_event(Event::MetadataIndexed(count));
                            *app = new_app;
                        }
                    }
                    Command::ExecuteSearch(where_clause) => {
                        if let Some(ref index) = metadata_index {
                            match index.search(&where_clause) {
                                Ok(results) => {
                                    let (new_app, _) = std::mem::take(app)
                                        .handle_event(Event::SearchResults(results));
                                    *app = new_app;
                                }
                                Err(e) => {
                                    let (new_app, _) = std::mem::take(app)
                                        .handle_event(Event::Error(format!("Search error: {}", e)));
                                    *app = new_app;
                                }
                            }
                        } else {
                            let (new_app, _) = std::mem::take(app)
                                .handle_event(Event::Error("Metadata not indexed yet".to_string()));
                            *app = new_app;
                        }
                    }
                }
            }

            if !app.running {
                break;
            }
        }
    }

    Ok(())
}

async fn handle_load_preview(
    app: &mut App,
    s3_client: &S3Client,
    picker: &mut Picker,
    image_state: &mut Option<StatefulProtocol>,
    pdf_raw_bytes: &mut Option<Vec<u8>>,
    bucket: &str,
    key: &str,
) {
    match s3_client
        .inner()
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
    {
        Ok(output) => match output.body.collect().await {
            Ok(bytes) => {
                let raw_bytes = bytes.into_bytes();

                if s3v::preview::pdf::is_pdf(key) {
                    // PDF プレビュー
                    match s3v::preview::pdf::page_count(&raw_bytes) {
                        Ok(total) => match s3v::preview::pdf::render_page(&raw_bytes, 0) {
                            Ok(first_page_png) => {
                                if let Ok(dyn_img) = image::load_from_memory(&first_page_png) {
                                    *image_state = Some(picker.new_resize_protocol(dyn_img));
                                }
                                *pdf_raw_bytes = Some(raw_bytes.to_vec());
                                let (new_app, _) = std::mem::take(app).handle_event(
                                    Event::PreviewLoaded(s3v::preview::PreviewContent::Pdf {
                                        pages: vec![first_page_png],
                                        current_page: 0,
                                        total_pages: total,
                                    }),
                                );
                                *app = new_app;
                            }
                            Err(e) => {
                                let (new_app, _) =
                                    std::mem::take(app).handle_event(Event::Error(e.to_string()));
                                *app = new_app;
                            }
                        },
                        Err(e) => {
                            let (new_app, _) =
                                std::mem::take(app).handle_event(Event::Error(e.to_string()));
                            *app = new_app;
                        }
                    }
                } else if s3v::preview::image::is_image(key) {
                    // 画像プレビュー
                    match image::load_from_memory(&raw_bytes) {
                        Ok(dyn_img) => {
                            *image_state = Some(picker.new_resize_protocol(dyn_img));
                            let (new_app, _) =
                                std::mem::take(app).handle_event(Event::PreviewLoaded(
                                    s3v::preview::PreviewContent::Image(raw_bytes.to_vec()),
                                ));
                            *app = new_app;
                        }
                        Err(e) => {
                            let (new_app, _) = std::mem::take(app)
                                .handle_event(Event::Error(format!("Image decode error: {}", e)));
                            *app = new_app;
                        }
                    }
                } else {
                    // テキストプレビュー
                    let raw = String::from_utf8_lossy(&raw_bytes).to_string();
                    let formatted = s3v::preview::text::format_preview(&raw, key);
                    let (new_app, _) = std::mem::take(app).handle_event(Event::PreviewLoaded(
                        s3v::preview::PreviewContent::Text(formatted),
                    ));
                    *app = new_app;
                }
            }
            Err(e) => {
                let (new_app, _) = std::mem::take(app).handle_event(Event::Error(e.to_string()));
                *app = new_app;
            }
        },
        Err(e) => {
            let (new_app, _) = std::mem::take(app).handle_event(Event::Error(e.to_string()));
            *app = new_app;
        }
    }
}
