use std::io;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

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

    // アプリケーション初期化
    let initial_path = cli.initial_path();
    let mut app = App::new();
    app.current_path = initial_path.clone();

    // バナー描画（初期ロード前に1フレーム描画）
    terminal.draw(|f| s3v::ui::render(&app, f))?;

    // 初期ロード
    let initial_items = s3_client.list(&initial_path).await.unwrap_or_default();
    let (new_app, _) = app.handle_event(Event::ItemsLoaded(initial_items));
    app = new_app;

    // メインループ
    let result = run_app(&mut terminal, &mut app, &s3_client).await;

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
) -> anyhow::Result<()> {
    loop {
        // 描画
        terminal.draw(|f| s3v::ui::render(app, f))?;

        // イベント待機
        if event::poll(Duration::from_millis(100))?
            && let CrosstermEvent::Key(key) = event::read()?
        {
            // KeyPress のみ処理（KeyRelease は無視）
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let event = match app.mode {
                s3v::Mode::Filter | s3v::Mode::Preview => Event::Key(key),
                _ => Event::from_key(key),
            };
            let (new_app, cmd) = std::mem::take(app).handle_event(event);
            *app = new_app;

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
                        match s3_client
                            .inner()
                            .get_object()
                            .bucket(&bucket)
                            .key(&key)
                            .send()
                            .await
                        {
                            Ok(output) => match output.body.collect().await {
                                Ok(bytes) => {
                                    let raw_bytes = bytes.into_bytes();
                                    let raw =
                                        String::from_utf8_lossy(&raw_bytes).to_string();
                                    let formatted =
                                        s3v::preview::text::format_preview(&raw, &key);
                                    let (new_app, _) = std::mem::take(app).handle_event(
                                        Event::PreviewLoaded(
                                            s3v::preview::PreviewContent::Text(formatted),
                                        ),
                                    );
                                    *app = new_app;
                                }
                                Err(e) => {
                                    let (new_app, _) = std::mem::take(app)
                                        .handle_event(Event::Error(e.to_string()));
                                    *app = new_app;
                                }
                            },
                            Err(e) => {
                                let (new_app, _) = std::mem::take(app)
                                    .handle_event(Event::Error(e.to_string()));
                                *app = new_app;
                            }
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
