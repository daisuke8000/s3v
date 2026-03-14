use crossterm::event::{KeyCode, KeyEvent};

use crate::command::Command;
use crate::preview::PreviewContent;
use crate::s3::{S3Item, S3Path};

use super::{App, BannerState, Mode, download, handle_text_input};

impl App {
    pub(crate) fn handle_key(mut self, key: KeyEvent) -> (Self, Vec<Command>) {
        // メッセージをクリア
        self.error_message = None;
        self.status_message = None;

        // Splash 状態では任意のキーで Active に遷移
        if self.banner_state == BannerState::Splash {
            return (
                Self {
                    banner_state: BannerState::Active,
                    ..self
                },
                vec![],
            );
        }

        match self.mode {
            Mode::Filter => self.handle_filter_key(key),
            Mode::PreviewFocus => self.handle_preview_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::DownloadConfirm => self.handle_download_confirm_key(key),
            Mode::Downloading => self.handle_downloading_key(key),
            _ => self.handle_normal_key(key),
        }
    }

    fn handle_normal_key(self, key: KeyEvent) -> (Self, Vec<Command>) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor_down(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.enter_item(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => self.go_back(),
            KeyCode::Char(' ') => (self.toggle_selection(), vec![]),
            KeyCode::Char('a') => (self.toggle_select_all(), vec![]),
            KeyCode::Char('/') => (self.enter_filter_mode(), vec![]),
            KeyCode::Char('d') => self.start_download(),
            KeyCode::Char('y') => self.copy_s3_uri(),
            KeyCode::Char('Y') => self.copy_https_url(),
            KeyCode::Char('?') => {
                let bucket = self.current_bucket();
                let prefix = self.current_path.prefix.clone();
                (
                    Self {
                        mode: Mode::Search,
                        search_query: String::new(),
                        indexing_in_progress: true,
                        ..self
                    },
                    vec![Command::IndexMetadata { bucket, prefix }],
                )
            }
            KeyCode::Tab => {
                // プレビューコンテンツがある場合は PreviewFocus に切替
                if self.preview_content.is_some() {
                    (
                        Self {
                            mode: Mode::PreviewFocus,
                            ..self
                        },
                        vec![],
                    )
                } else {
                    (self, vec![])
                }
            }
            _ => (self, vec![]),
        }
    }

    fn handle_filter_key(mut self, key: KeyEvent) -> (Self, Vec<Command>) {
        match key.code {
            KeyCode::Enter => (self.apply_filter(), vec![]),
            KeyCode::Esc => (self.clear_filter(), vec![]),
            _ => {
                handle_text_input(&mut self.filter, key);
                (self, vec![])
            }
        }
    }

    fn handle_preview_key(self, key: KeyEvent) -> (Self, Vec<Command>) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Tab => (
                Self {
                    mode: Mode::Normal,
                    ..self
                },
                vec![],
            ),
            KeyCode::Down | KeyCode::Char('j') => (
                Self {
                    preview_scroll: self.preview_scroll.saturating_add(1),
                    ..self
                },
                vec![],
            ),
            KeyCode::Up | KeyCode::Char('k') => (
                Self {
                    preview_scroll: self.preview_scroll.saturating_sub(1),
                    ..self
                },
                vec![],
            ),
            KeyCode::PageDown => (
                Self {
                    preview_scroll: self.preview_scroll.saturating_add(10),
                    ..self
                },
                vec![],
            ),
            KeyCode::PageUp => (
                Self {
                    preview_scroll: self.preview_scroll.saturating_sub(10),
                    ..self
                },
                vec![],
            ),
            KeyCode::Right | KeyCode::Char('l') => (self.next_pdf_page(), vec![]),
            KeyCode::Left | KeyCode::Char('h') => (self.prev_pdf_page(), vec![]),
            _ => (self, vec![]),
        }
    }

    fn next_pdf_page(self) -> Self {
        if let Some(PreviewContent::Pdf {
            current_page,
            total_pages,
        }) = &self.preview_content
            && current_page + 1 < *total_pages
        {
            return Self {
                preview_content: Some(PreviewContent::Pdf {
                    current_page: current_page + 1,
                    total_pages: *total_pages,
                }),
                ..self
            };
        }
        self
    }

    fn prev_pdf_page(self) -> Self {
        if let Some(PreviewContent::Pdf {
            current_page,
            total_pages,
        }) = &self.preview_content
            && *current_page > 0
        {
            return Self {
                preview_content: Some(PreviewContent::Pdf {
                    current_page: current_page - 1,
                    total_pages: *total_pages,
                }),
                ..self
            };
        }
        self
    }

    fn handle_download_confirm_key(self, key: KeyEvent) -> (Self, Vec<Command>) {
        match key.code {
            KeyCode::Esc => (
                Self {
                    mode: Mode::Normal,
                    download_target: None,
                    ..self
                },
                vec![],
            ),
            KeyCode::Up | KeyCode::Down => {
                let focus = match self.confirm_focus {
                    download::ConfirmFocus::Path => download::ConfirmFocus::Buttons,
                    download::ConfirmFocus::Buttons => download::ConfirmFocus::Path,
                };
                (
                    Self {
                        confirm_focus: focus,
                        path_completions: Vec::new(),
                        completion_index: 0,
                        ..self
                    },
                    vec![],
                )
            }
            KeyCode::Left | KeyCode::Right
                if self.confirm_focus == download::ConfirmFocus::Buttons =>
            {
                let btn = match self.confirm_button {
                    download::ConfirmButton::Start => download::ConfirmButton::Cancel,
                    download::ConfirmButton::Cancel => download::ConfirmButton::Start,
                };
                (
                    Self {
                        confirm_button: btn,
                        ..self
                    },
                    vec![],
                )
            }
            KeyCode::Enter if self.confirm_focus == download::ConfirmFocus::Buttons => {
                match self.confirm_button {
                    download::ConfirmButton::Cancel => (
                        Self {
                            mode: Mode::Normal,
                            download_target: None,
                            ..self
                        },
                        vec![],
                    ),
                    download::ConfirmButton::Start => self.execute_download(),
                }
            }
            KeyCode::Tab if self.confirm_focus == download::ConfirmFocus::Path => {
                self.cycle_path_completion()
            }
            _ if self.confirm_focus == download::ConfirmFocus::Path => {
                let mut new_self = self;
                handle_text_input(&mut new_self.download_path, key);
                new_self.path_completions.clear();
                new_self.completion_index = 0;
                (new_self, vec![])
            }
            _ => (self, vec![]),
        }
    }

    fn handle_downloading_key(self, key: KeyEvent) -> (Self, Vec<Command>) {
        match key.code {
            KeyCode::Esc => (
                Self {
                    mode: Mode::Normal,
                    download_progress: None,
                    download_target: None,
                    ..self
                },
                vec![Command::CancelDownload],
            ),
            _ => (self, vec![]),
        }
    }

    pub fn item_to_s3_path(&self, item: &S3Item) -> S3Path {
        let bucket = self.current_bucket();
        match item {
            S3Item::Bucket { name } => S3Path::bucket(name),
            S3Item::Folder { prefix, .. } => S3Path::with_prefix(&bucket, prefix),
            S3Item::File { key, .. } => S3Path::with_prefix(&bucket, key),
        }
    }

    fn copy_s3_uri(self) -> (Self, Vec<Command>) {
        if let Some(item) = self.selected_item().cloned() {
            let uri = self.item_to_s3_path(&item).to_s3_uri();
            (self, vec![Command::CopyToClipboard(uri)])
        } else {
            (self, vec![])
        }
    }

    fn copy_https_url(self) -> (Self, Vec<Command>) {
        if let Some(item) = self.selected_item().cloned() {
            let url = self.item_to_s3_path(&item).to_https_url(&self.region);
            (self, vec![Command::CopyToClipboard(url)])
        } else {
            (self, vec![])
        }
    }

    fn handle_search_key(mut self, key: KeyEvent) -> (Self, Vec<Command>) {
        match key.code {
            KeyCode::Enter => {
                let query = self.search_query.clone();
                (
                    Self {
                        mode: Mode::Loading,
                        ..self
                    },
                    vec![Command::ExecuteSearch(query)],
                )
            }
            KeyCode::Esc => (
                Self {
                    mode: Mode::Normal,
                    search_query: String::new(),
                    ..self
                },
                vec![],
            ),
            _ => {
                handle_text_input(&mut self.search_query, key);
                (self, vec![])
            }
        }
    }
}
