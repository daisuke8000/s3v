use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tokio::sync::{Semaphore, mpsc};

use crate::App;
use crate::command::Command;
use crate::command_handler::{PreviewState, dispatch_event, start_prefetch, start_preview_load};
use crate::event::Event;
use crate::runtime::{DebounceState, RuntimeState};
use crate::s3::S3Client;

/// コマンド実行に必要なランタイムコンテキスト
pub struct CommandContext {
    pub metadata_index: Option<Arc<crate::search::MetadataIndex>>,
    pub indexing_prefix: Option<String>,
    pub debounce: DebounceState,
    pub runtime: RuntimeState,
}

/// コマンドリストを順次実行
pub async fn handle_commands(
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
                            let result = crate::download::download_file_with_structure(
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
                                    let _ = tx.send(Event::Error(crate::error::user_error(
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
                                    crate::S3Item::File { size, .. } => Some(*size),
                                    _ => None,
                                })
                                .sum();
                            let _ = tx.send(Event::FolderFilesListed { files, total_size });
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(crate::error::user_error(
                                "List folder failed",
                                e,
                            )));
                        }
                    }
                });
            }
            Command::CancelDownload => {
                dispatch_event(app, Event::Error("Download cancelled".to_string()));
            }
            Command::LoadPreview { bucket, key } => {
                start_preview_load(app, s3_client, preview, &bucket, &key, stream_tx);
            }
            Command::LoadItems(path) => match s3_client.list(&path).await {
                Ok(result) => {
                    dispatch_event(
                        app,
                        Event::ItemsLoaded {
                            items: result.items,
                            next_token: result.next_token,
                        },
                    );

                    let auto_cmds = {
                        let mut temp_app = std::mem::take(app);
                        let cmds = temp_app.build_auto_preview_commands();
                        *app = temp_app;
                        cmds
                    };
                    for auto_cmd in auto_cmds {
                        handle_single_command(app, s3_client, preview, ctx, stream_tx, auto_cmd)
                            .await?;
                    }
                }
                Err(e) => {
                    dispatch_event(app, Event::Error(crate::error::user_error("S3 error", e)));
                }
            },
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
                            let _ = tx.send(Event::Error(crate::error::user_error(
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
                    let path = crate::S3Path::with_prefix(&bucket, &prefix);
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
                let tx = stream_tx.clone();
                ctx.debounce
                    .schedule(debounce_key.clone(), bucket, key.clone(), tx);

                app.pending_preview_key = Some(debounce_key);

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
            Command::CopyToClipboard(text) => match crate::clipboard::copy_to_clipboard(&text) {
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
                if ctx.indexing_prefix.as_deref() == Some(&prefix) {
                    return Ok(());
                }

                if ctx.metadata_index.is_none() {
                    match crate::search::MetadataIndex::open(&bucket) {
                        Ok(index) => {
                            ctx.metadata_index = Some(Arc::new(index));
                        }
                        Err(e) => {
                            dispatch_event(
                                app,
                                Event::Error(crate::error::user_error("Index open failed", e)),
                            );
                            return Ok(());
                        }
                    }
                }
                let index = ctx.metadata_index.clone().expect("index initialized above");

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
                                tx.send(Event::Error(crate::error::user_error("Index failed", e)));
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
                let ts = crate::download::zip_download::download_timestamp();
                let zip_path = crate::download::zip_download::zip_destination(
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

                    let result = crate::download::zip_download::download_as_zip(
                        &client,
                        &bucket,
                        &keys,
                        &zip_path,
                        &base_prefix,
                        cancel,
                        progress_tx,
                    )
                    .await;

                    let _ = progress_handle.await;

                    match result {
                        Ok(()) => {
                            let _ = tx.send(Event::DownloadAllComplete { count: keys.len() });
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(crate::error::user_error(
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
                                let _ = tx.send(Event::Error(crate::error::user_error(
                                    "Search error",
                                    e,
                                )));
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
