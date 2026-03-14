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

    // ストリーミングイベント用チャネル
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<Event>();

    // crossterm EventStream（非同期キーイベント）
    let mut event_stream = EventStream::new();

    loop {
        // メッセージの自動消去チェック（3秒後）
        if ctx.runtime.should_dismiss_error() {
            app.error_message = None;
        }
        if ctx.runtime.should_dismiss_status() {
            app.status_message = None;
        }

        // PDF ページ変更検知 → 再レンダリング（PDF 表示中のみ）
        if matches!(
            app.preview_content,
            Some(s3v::preview::PreviewContent::Pdf { .. })
        ) {
            update_pdf_page(app, &mut preview, picker).await;
        }

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
                // インデックス構築完了 or エラー時に indexing_prefix をクリア
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

                    // プレビューを維持しないモードに遷移したらクリア
                    if !app.mode.preserves_preview() {
                        preview.clear();
                    }

                    // Loading 状態なら即座に再描画
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

/// コマンド実行に必要なランタイムコンテキスト
struct CommandContext {
    metadata_index: Option<Arc<s3v::search::MetadataIndex>>,
    indexing_prefix: Option<String>,
    debounce: DebounceState,
    runtime: s3v::runtime::RuntimeState,
}

/// コマンド実行（副作用はここで処理）
async fn handle_commands(
    app: &mut App,
    s3_client: &S3Client,
    preview: &mut PreviewState,
    ctx: &mut CommandContext,
    stream_tx: &mpsc::UnboundedSender<Event>,
    cmds: Vec<Command>,
) -> anyhow::Result<()> {
    for cmd in cmds {
        handle_single_command(app, s3_client, preview, ctx, stream_tx, cmd).await?;
    }
    Ok(())
}

fn handle_single_command<'a>(
    app: &'a mut App,
    s3_client: &'a S3Client,
    preview: &'a mut PreviewState,
    ctx: &'a mut CommandContext,
    stream_tx: &'a mpsc::UnboundedSender<Event>,
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
                    Ok(result) => {
                        dispatch_event(
                            app,
                            Event::ItemsLoaded {
                                items: result.items,
                                next_token: result.next_token,
                            },
                        );

                        // アイテム読み込み後、カーソル位置の自動プレビューをトリガー
                        let auto_cmds = {
                            let mut temp_app = std::mem::take(app);
                            let cmds = temp_app.build_auto_preview_commands();
                            *app = temp_app;
                            cmds
                        };
                        for auto_cmd in auto_cmds {
                            handle_single_command(
                                app, s3_client, preview, ctx, stream_tx, auto_cmd,
                            )
                            .await?;
                        }
                    }
                    Err(e) => {
                        dispatch_event(app, Event::Error(s3v::error::user_error("S3 error", e)));
                    }
                }
            }
            Command::LoadMore { path, token } => {
                let s3_client = s3_client.clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    match s3_client.list_objects_continued(&path, &token).await {
                        Ok(result) => {
                            let _ = tx.send(Event::MoreItemsLoaded {
                                items: result.items,
                                next_token: result.next_token,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(s3v::error::user_error(
                                "Failed to load more items",
                                e,
                            )));
                        }
                    }
                });
            }
            Command::LoadParentItems(path) => {
                let s3_client = s3_client.clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    if let Ok(result) = s3_client.list(&path).await {
                        let _ = tx.send(Event::ParentItemsLoaded(result.items));
                    }
                });
            }
            Command::LoadFolderPreview { bucket, prefix } => {
                let s3_client = s3_client.clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    let path = s3v::S3Path::with_prefix(&bucket, &prefix);
                    if let Ok(result) = s3_client.list(&path).await {
                        let _ = tx.send(Event::FolderPreviewLoaded(result.items));
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
                ctx.debounce
                    .schedule(debounce_key.clone(), bucket, key.clone(), tx);

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
            Command::CopyToClipboard(text) => match s3v::clipboard::copy_to_clipboard(&text) {
                Ok(()) => {
                    app.status_message = Some("Copied to clipboard".to_string());
                    ctx.runtime.mark_status_shown();
                }
                Err(e) => {
                    app.error_message = Some(format!("Clipboard: {}", e));
                    ctx.runtime.mark_error_shown();
                }
            },
            Command::IndexMetadata { bucket, prefix } => {
                // 重複チェック: 同じ prefix が構築中なら無視
                if ctx.indexing_prefix.as_deref() == Some(&prefix) {
                    return Ok(());
                }

                // MetadataIndex が未初期化なら開く
                if ctx.metadata_index.is_none() {
                    match s3v::search::MetadataIndex::open(&bucket) {
                        Ok(index) => {
                            ctx.metadata_index = Some(Arc::new(index));
                        }
                        Err(e) => {
                            dispatch_event(
                                app,
                                Event::Error(s3v::error::user_error("Index open failed", e)),
                            );
                            return Ok(());
                        }
                    }
                }
                let index = ctx.metadata_index.clone().expect("index initialized above");

                // 既にカバー済みなら構築不要
                if index.is_prefix_covered(&prefix).unwrap_or(false) {
                    dispatch_event(app, Event::MetadataIndexed(0));
                    return Ok(());
                }

                ctx.indexing_prefix = Some(prefix.clone());
                let s3_client = s3_client.clone();
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    match s3_client.list_all_files(&bucket, &prefix).await {
                        Ok(items) => {
                            let count = index.insert_items(&items).unwrap_or(0);
                            index.mark_prefix_indexed(&prefix).unwrap_or(());
                            let _ = tx.send(Event::MetadataIndexed(count));
                        }
                        Err(e) => {
                            let _ =
                                tx.send(Event::Error(s3v::error::user_error("Index failed", e)));
                        }
                    }
                });
            }
            Command::StartZipDownload {
                bucket,
                keys,
                destination,
                base_prefix,
                archive_name,
                total_size,
            } => {
                // Timestamp generation happens here in main.rs (runtime side-effect)
                let ts = s3v::download::zip_download::download_timestamp();
                let zip_path = s3v::download::zip_download::zip_destination(
                    &destination,
                    &archive_name,
                    total_size,
                    &ts,
                );

                let client = s3_client.inner().clone();
                let tx = stream_tx.clone();
                let cancel = Arc::new(AtomicBool::new(false));
                tokio::spawn(async move {
                    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();

                    let tx_clone = tx.clone();
                    let progress_handle = tokio::spawn(async move {
                        while let Some((completed, total, file_name)) = progress_rx.recv().await {
                            let _ = tx_clone.send(Event::DownloadFileComplete {
                                completed,
                                total,
                                current_file: file_name,
                            });
                        }
                    });

                    let result = s3v::download::zip_download::download_as_zip(
                        &client,
                        &bucket,
                        &keys,
                        &zip_path,
                        &base_prefix,
                        cancel,
                        progress_tx,
                    )
                    .await;

                    // Wait for progress forwarder to finish
                    let _ = progress_handle.await;

                    match result {
                        Ok(()) => {
                            let _ = tx.send(Event::DownloadAllComplete { count: keys.len() });
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(s3v::error::user_error(
                                "Zip download failed",
                                e,
                            )));
                        }
                    }
                });
            }
            Command::ExecuteSearch(where_clause) => {
                if let Some(index) = ctx.metadata_index.clone() {
                    let prefix = app.current_path.prefix.clone();
                    let tx = stream_tx.clone();
                    tokio::spawn(async move {
                        match index.search(&prefix, &where_clause) {
                            Ok(results) => {
                                let _ = tx.send(Event::SearchResults(results));
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Event::Error(s3v::error::user_error("Search error", e)));
                            }
                        }
                    });
                } else {
                    dispatch_event(
                        app,
                        Event::Error("Index not available. Press ? to build.".to_string()),
                    );
                }
            }
        }

        Ok(())
    })
}
