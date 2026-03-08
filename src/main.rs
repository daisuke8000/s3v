use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use clap::Parser;
use crossterm::{
    event::{EventStream, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use ratatui_image::picker::Picker;
use tokio::sync::{Semaphore, mpsc};

use s3v::command_handler::{
    PreviewState, dispatch_event, setup_pdf_worker, start_debounced_preview, start_prefetch,
    start_preview_load, update_pdf_page,
};
use s3v::{App, Cli, Command, Event, S3Client};

/// デバウンス状態（main.rs のランタイムで管理）
struct DebounceState {
    pending_key: Option<String>,
    /// デバウンス対象の bucket
    pending_bucket: String,
    /// デバウンス対象の key
    pending_obj_key: String,
    timer_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DebounceState {
    fn new() -> Self {
        Self {
            pending_key: None,
            pending_bucket: String::new(),
            pending_obj_key: String::new(),
            timer_handle: None,
        }
    }

    /// 新しいデバウンス要求をセット（前のタイマーをキャンセル）
    fn schedule(
        &mut self,
        debounce_key: String,
        bucket: String,
        obj_key: String,
        debounce_tx: mpsc::UnboundedSender<Event>,
    ) {
        // 前のタイマーをキャンセル
        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
        }

        self.pending_key = Some(debounce_key.clone());
        self.pending_bucket = bucket;
        self.pending_obj_key = obj_key;

        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = debounce_tx.send(Event::DebounceTimeout { debounce_key });
        });

        self.timer_handle = Some(handle);
    }
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
    let mut preview = PreviewState::new();
    let mut metadata_index: Option<s3v::search::MetadataIndex> = None;
    let mut debounce = DebounceState::new();

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
                if matches!(&stream_event, Event::PreviewImageReady)
                    && let Some(dyn_img) = preview.take_decoded_image()
                {
                    preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                }
                if matches!(&stream_event, Event::PdfDataReady) {
                    setup_pdf_worker(app, picker, &mut preview).await;
                }
                // DebounceTimeout: デバウンスキーが一致すればプレビュー開始
                if let Event::DebounceTimeout { ref debounce_key } = stream_event
                    && debounce.pending_key.as_deref() == Some(debounce_key)
                {
                    let bucket = debounce.pending_bucket.clone();
                    let obj_key = debounce.pending_obj_key.clone();
                    let dk = debounce_key.clone();
                    start_debounced_preview(
                        app, s3_client, &mut preview,
                        &bucket, &obj_key, &dk, &stream_tx,
                    );
                }
                let cmds = dispatch_event(app, stream_event);
                handle_commands(app, s3_client, &mut preview, &mut metadata_index, &stream_tx, &mut debounce, cmds).await?;
            }
            Some(Ok(crossterm_event)) = event_stream.next() => {
                if let crossterm::event::Event::Key(key) = crossterm_event {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    let event = match app.mode {
                        s3v::Mode::Filter
                        | s3v::Mode::PreviewFocus
                        | s3v::Mode::Search
                        | s3v::Mode::DownloadConfirm => Event::Key(key),
                        _ => Event::from_key(key),
                    };
                    let cmds = dispatch_event(app, event);

                    // PreviewFocus モードを抜けたらプレビュー状態をクリア
                    if app.mode != s3v::Mode::PreviewFocus
                        && app.mode != s3v::Mode::Normal
                        && app.mode != s3v::Mode::Loading
                        && app.mode != s3v::Mode::DownloadConfirm
                        && app.mode != s3v::Mode::Downloading
                    {
                        preview.clear();
                    }

                    // Loading 状態なら即座に再描画
                    if app.mode == s3v::Mode::Loading {
                        terminal.draw(|f| s3v::ui::render(app, f, preview.image_state.as_mut()))?;
                    }

                    handle_commands(app, s3_client, &mut preview, &mut metadata_index, &stream_tx, &mut debounce, cmds).await?;
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
async fn handle_commands(
    app: &mut App,
    s3_client: &S3Client,
    preview: &mut PreviewState,
    metadata_index: &mut Option<s3v::search::MetadataIndex>,
    stream_tx: &mpsc::UnboundedSender<Event>,
    debounce: &mut DebounceState,
    cmds: Vec<Command>,
) -> anyhow::Result<()> {
    for cmd in cmds {
        handle_single_command(
            app,
            s3_client,
            preview,
            metadata_index,
            stream_tx,
            debounce,
            cmd,
        )
        .await?;
    }
    Ok(())
}

fn handle_single_command<'a>(
    app: &'a mut App,
    s3_client: &'a S3Client,
    preview: &'a mut PreviewState,
    metadata_index: &'a mut Option<s3v::search::MetadataIndex>,
    stream_tx: &'a mpsc::UnboundedSender<Event>,
    debounce: &'a mut DebounceState,
    cmd: Command,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + 'a>> {
    Box::pin(async move {
        match cmd {
            Command::Quit => {
                app.running = false;
            }
            Command::StartDownload {
                bucket,
                keys,
                destination,
                base_prefix,
            } => {
                let client = s3_client.inner().clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    let semaphore = Arc::new(Semaphore::new(4));
                    let total = keys.len();
                    let completed = Arc::new(AtomicUsize::new(0));
                    let cancel = Arc::new(AtomicBool::new(false));
                    let mut handles = Vec::new();

                    for key in keys {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(p) => p,
                            Err(_) => break,
                        };
                        let client = client.clone();
                        let bucket = bucket.clone();
                        let dest = destination.clone();
                        let tx = tx.clone();
                        let completed = completed.clone();
                        let base_prefix = base_prefix.clone();

                        handles.push(tokio::spawn(async move {
                            let _permit = permit;
                            let result = s3v::download::download_file_with_structure(
                                &client,
                                &bucket,
                                &key,
                                &dest,
                                &base_prefix,
                            )
                            .await;
                            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                            let file_name = key.split('/').next_back().unwrap_or(&key).to_string();

                            match result {
                                Ok(()) => {
                                    let _ = tx.send(Event::DownloadFileComplete {
                                        completed: done,
                                        total,
                                        current_file: file_name,
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(Event::Error(s3v::error::user_error(
                                        &format!("Download failed: {}", key),
                                        e,
                                    )));
                                }
                            }
                        }));
                    }

                    for handle in handles {
                        let _ = handle.await;
                    }
                    let _ = tx.send(Event::DownloadAllComplete {
                        count: completed.load(Ordering::Relaxed),
                    });
                });
            }
            Command::ListFolderFiles { bucket, prefix } => {
                let s3_client = s3_client.clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    match s3_client.list_all_files(&bucket, &prefix).await {
                        Ok(files) => {
                            let total_size: u64 = files
                                .iter()
                                .filter_map(|f| match f {
                                    s3v::S3Item::File { size, .. } => Some(*size),
                                    _ => None,
                                })
                                .sum();
                            let _ = tx.send(Event::FolderFilesListed { files, total_size });
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(s3v::error::user_error(
                                "List folder failed",
                                e,
                            )));
                        }
                    }
                });
            }
            Command::CancelDownload => {
                // DL キャンセル — Normal に戻る
                dispatch_event(app, Event::Error("Download cancelled".to_string()));
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

                        // アイテム読み込み後、カーソル位置の自動プレビューをトリガー
                        let auto_cmds = {
                            let mut temp_app = std::mem::take(app);
                            let cmds = temp_app.build_auto_preview_commands();
                            *app = temp_app;
                            cmds
                        };
                        for auto_cmd in auto_cmds {
                            handle_single_command(
                                app,
                                s3_client,
                                preview,
                                metadata_index,
                                stream_tx,
                                debounce,
                                auto_cmd,
                            )
                            .await?;
                        }
                    }
                    Err(e) => {
                        dispatch_event(app, Event::Error(s3v::error::user_error("S3 error", e)));
                    }
                }
            }
            Command::LoadParentItems(path) => {
                let s3_client = s3_client.clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    if let Ok(items) = s3_client.list(&path).await {
                        let _ = tx.send(Event::ParentItemsLoaded(items));
                    }
                });
            }
            Command::LoadFolderPreview { bucket, prefix } => {
                let s3_client = s3_client.clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    let path = s3v::S3Path::with_prefix(&bucket, &prefix);
                    if let Ok(items) = s3_client.list(&path).await {
                        let _ = tx.send(Event::FolderPreviewLoaded(items));
                    }
                });
            }
            Command::RequestPreview {
                bucket,
                key,
                debounce_key,
            } => {
                // デバウンスタイマーを設定（bucket/key も保存）
                let tx = stream_tx.clone();
                debounce.schedule(debounce_key.clone(), bucket, key.clone(), tx);

                app.pending_preview_key = Some(debounce_key);

                // フォルダ→ファイル遷移時のちらつき防止
                if !key.is_empty() && !key.ends_with('/') {
                    app.folder_preview_items.clear();
                }
            }
            Command::PrefetchPreview { bucket, key } => {
                start_prefetch(s3_client, stream_tx, &bucket, &key);
            }
            Command::CancelPreview => {
                preview.cancel_stream();
                app.pending_preview_key = None;
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
            Command::StartZipDownload { .. } => {
                // TODO: Task 4 で実装
            }
            Command::ExecuteSearch(where_clause) => {
                if let Some(index) = metadata_index.as_ref() {
                    match index.search(&where_clause) {
                        Ok(results) => {
                            dispatch_event(app, Event::SearchResults(results));
                        }
                        Err(e) => {
                            dispatch_event(
                                app,
                                Event::Error(s3v::error::user_error("Search error", e)),
                            );
                        }
                    }
                } else {
                    dispatch_event(app, Event::Error("Metadata not indexed yet".to_string()));
                }
            }
        }

        Ok(())
    })
}
