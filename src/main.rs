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

/// App にイベントを送信し、状態を更新するヘルパー
fn dispatch_event(app: &mut App, event: Event) -> Option<Command> {
    let (new_app, cmd) = std::mem::take(app).handle_event(event);
    *app = new_app;
    cmd
}

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
    let mut preview = PreviewState {
        image_state: None,
        pdf_raw_bytes: None,
        last_pdf_page: None,
    };
    let mut metadata_index: Option<s3v::search::MetadataIndex> = None;

    loop {
        // PDF ページ変更検知 → 再レンダリング
        if let Some(s3v::preview::PreviewContent::Pdf {
            current_page,
            total_pages: _,
        }) = &app.preview_content
        {
            let current = *current_page;
            if preview.last_pdf_page != Some(current)
                && let Some(ref raw) = preview.pdf_raw_bytes
            {
                let bytes_clone = raw.clone();
                let page = current;
                let result = tokio::task::spawn_blocking(move || {
                    s3v::preview::pdf::render_page_to_image(&bytes_clone, page)
                })
                .await;
                match result {
                    Ok(Ok(dyn_img)) => {
                        preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                    }
                    Ok(Err(e)) => {
                        dispatch_event(app, Event::Error(format!("PDF render error: {}", e)));
                    }
                    Err(e) => {
                        dispatch_event(app, Event::Error(format!("PDF task error: {}", e)));
                    }
                }
                preview.last_pdf_page = Some(current);
            }
        }

        // 描画
        terminal.draw(|f| s3v::ui::render(app, f, preview.image_state.as_mut()))?;

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
            let cmd = dispatch_event(app, event);

            // Preview モードを抜けたらプレビュー状態をクリア
            if app.mode != s3v::Mode::Preview {
                preview.image_state = None;
                preview.pdf_raw_bytes = None;
                preview.last_pdf_page = None;
            }

            // Loading 状態なら即座に再描画（ブロッキング処理前に表示更新）
            if app.mode == s3v::Mode::Loading {
                terminal.draw(|f| s3v::ui::render(app, f, preview.image_state.as_mut()))?;
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
                            dispatch_event(app, Event::Error(format!("Download error: {}", e)));
                        }
                    }
                    Command::LoadPreview { bucket, key } => {
                        handle_load_preview(app, s3_client, picker, &mut preview, &bucket, &key)
                            .await;
                    }
                    Command::LoadItems(path) => {
                        match s3_client.list(&path).await {
                            Ok(items) => {
                                dispatch_event(app, Event::ItemsLoaded(items));

                                // バケットに入った時にメタデータインデックスを構築
                                if let Some(bucket) = &app.current_path.bucket
                                    && !app.metadata_indexed
                                    && let Ok(all_items) =
                                        s3_client.list_all_objects(bucket).await
                                    && let Ok(index) = s3v::search::MetadataIndex::new()
                                    && let Ok(count) = index.insert_items(&all_items)
                                {
                                    metadata_index = Some(index);
                                    dispatch_event(app, Event::MetadataIndexed(count));
                                }
                            }
                            Err(e) => {
                                dispatch_event(
                                    app,
                                    Event::Error(format!("Failed to load items: {}", e)),
                                );
                            }
                        }
                    }
                    Command::IndexMetadata { bucket } => {
                        if let Ok(all_items) = s3_client.list_all_objects(&bucket).await
                            && let Ok(index) = s3v::search::MetadataIndex::new()
                            && let Ok(count) = index.insert_items(&all_items)
                        {
                            metadata_index = Some(index);
                            dispatch_event(app, Event::MetadataIndexed(count));
                        }
                    }
                    Command::ExecuteSearch(where_clause) => {
                        if let Some(ref index) = metadata_index {
                            match index.search(&where_clause) {
                                Ok(results) => {
                                    dispatch_event(app, Event::SearchResults(results));
                                }
                                Err(e) => {
                                    dispatch_event(
                                        app,
                                        Event::Error(format!("Search error: {}", e)),
                                    );
                                }
                            }
                        } else {
                            dispatch_event(
                                app,
                                Event::Error("Metadata not indexed yet".to_string()),
                            );
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

/// プレビュー描画用のランタイム状態（App に入れられない non-Clone 型を管理）
struct PreviewState {
    image_state: Option<StatefulProtocol>,
    pdf_raw_bytes: Option<Vec<u8>>,
    last_pdf_page: Option<usize>,
}

async fn handle_load_preview(
    app: &mut App,
    s3_client: &S3Client,
    picker: &mut Picker,
    preview: &mut PreviewState,
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
                    // PDF 画像レンダリングプレビュー（spawn_blocking でブロッキング回避）
                    let pdf_bytes = raw_bytes.to_vec();
                    let bytes_for_count = pdf_bytes.clone();
                    let bytes_for_render = pdf_bytes.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let total = s3v::preview::pdf::page_count(&bytes_for_count)?;
                        let img = s3v::preview::pdf::render_page_to_image(&bytes_for_render, 0)?;
                        Ok::<(usize, image::DynamicImage), s3v::error::S3vError>((total, img))
                    })
                    .await;
                    match result {
                        Ok(Ok((total_pages, dyn_img))) => {
                            preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                            preview.pdf_raw_bytes = Some(pdf_bytes);
                            preview.last_pdf_page = Some(0);
                            dispatch_event(
                                app,
                                Event::PreviewLoaded(s3v::preview::PreviewContent::Pdf {
                                    current_page: 0,
                                    total_pages,
                                }),
                            );
                        }
                        Ok(Err(e)) => {
                            dispatch_event(app, Event::Error(e.to_string()));
                        }
                        Err(e) => {
                            dispatch_event(
                                app,
                                Event::Error(format!("PDF task error: {}", e)),
                            );
                        }
                    }
                } else if s3v::preview::image::is_image(key) {
                    // 画像プレビュー
                    match image::load_from_memory(&raw_bytes) {
                        Ok(dyn_img) => {
                            preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                            dispatch_event(
                                app,
                                Event::PreviewLoaded(s3v::preview::PreviewContent::Image(
                                    raw_bytes.to_vec(),
                                )),
                            );
                        }
                        Err(e) => {
                            dispatch_event(
                                app,
                                Event::Error(format!("Image decode error: {}", e)),
                            );
                        }
                    }
                } else {
                    // テキストプレビュー
                    let raw = String::from_utf8_lossy(&raw_bytes).to_string();
                    let formatted = s3v::preview::text::format_preview(&raw, key);
                    dispatch_event(
                        app,
                        Event::PreviewLoaded(s3v::preview::PreviewContent::Text(formatted)),
                    );
                }
            }
            Err(e) => {
                dispatch_event(app, Event::Error(e.to_string()));
            }
        },
        Err(e) => {
            dispatch_event(app, Event::Error(e.to_string()));
        }
    }
}
