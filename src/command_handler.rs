use crate::app::App;
use crate::command::Command;
use crate::event::Event;
use crate::s3::S3Client;

use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

/// プレビュー描画用のランタイム状態（App に入れられない non-Clone 型を管理）
pub struct PreviewState {
    pub image_state: Option<StatefulProtocol>,
    pub pdf_raw_bytes: Option<Vec<u8>>,
    pub last_pdf_page: Option<usize>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewState {
    pub fn new() -> Self {
        Self {
            image_state: None,
            pdf_raw_bytes: None,
            last_pdf_page: None,
        }
    }

    pub fn clear(&mut self) {
        self.image_state = None;
        self.pdf_raw_bytes = None;
        self.last_pdf_page = None;
    }
}

/// App にイベントを送信し、状態を更新するヘルパー
pub fn dispatch_event(app: &mut App, event: Event) -> Option<Command> {
    let (new_app, cmd) = std::mem::take(app).handle_event(event);
    *app = new_app;
    cmd
}

/// PDF ページ変更を検知して再レンダリング
pub async fn update_pdf_page(app: &mut App, preview: &mut PreviewState, picker: &mut Picker) {
    if let Some(crate::preview::PreviewContent::Pdf {
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
                crate::preview::pdf::render_page_to_image(&bytes_clone, page)
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
}

/// LoadPreview コマンドの処理
pub async fn handle_load_preview(
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

                if crate::preview::pdf::is_pdf(key) {
                    // PDF 画像レンダリングプレビュー（spawn_blocking でブロッキング回避）
                    let pdf_bytes = raw_bytes.to_vec();
                    let bytes_for_count = pdf_bytes.clone();
                    let bytes_for_render = pdf_bytes.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let total = crate::preview::pdf::page_count(&bytes_for_count)?;
                        let img =
                            crate::preview::pdf::render_page_to_image(&bytes_for_render, 0)?;
                        Ok::<(usize, image::DynamicImage), crate::error::S3vError>((total, img))
                    })
                    .await;
                    match result {
                        Ok(Ok((total_pages, dyn_img))) => {
                            preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                            preview.pdf_raw_bytes = Some(pdf_bytes);
                            preview.last_pdf_page = Some(0);
                            dispatch_event(
                                app,
                                Event::PreviewLoaded(crate::preview::PreviewContent::Pdf {
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
                } else if crate::preview::image::is_image(key) {
                    // 画像プレビュー
                    match image::load_from_memory(&raw_bytes) {
                        Ok(dyn_img) => {
                            preview.image_state = Some(picker.new_resize_protocol(dyn_img));
                            dispatch_event(
                                app,
                                Event::PreviewLoaded(crate::preview::PreviewContent::Image(
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
                    let formatted = crate::preview::text::format_preview(&raw, key);
                    dispatch_event(
                        app,
                        Event::PreviewLoaded(crate::preview::PreviewContent::Text(formatted)),
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
