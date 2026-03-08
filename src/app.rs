use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent};

use crate::command::Command;
use crate::event::Event;
use crate::preview::PreviewContent;
use crate::s3::{S3Item, S3Path};

/// アプリケーションモード
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Loading,
    Filter,
    Preview,
    Search,
}

/// アプリケーション状態（Model）
#[derive(Debug, Clone)]
pub struct App {
    /// 現在のパス
    pub current_path: S3Path,
    /// 表示中のアイテム一覧
    pub items: Vec<S3Item>,
    /// カーソル位置
    pub cursor: usize,
    /// アプリケーションモード
    pub mode: Mode,
    /// 実行中フラグ
    pub running: bool,
    /// 起動バナー表示フラグ
    pub show_banner: bool,
    /// 選択されたアイテムのインデックス
    pub selected: HashSet<usize>,
    /// フィルタ文字列
    pub filter: String,
    /// フィルタ適用前の全アイテム
    pub all_items: Vec<S3Item>,
    /// プレビューコンテンツ
    pub preview_content: Option<PreviewContent>,
    /// プレビューのスクロール位置
    pub preview_scroll: u16,
    /// 検索クエリ
    pub search_query: String,
    /// メタデータインデックス済みフラグ
    pub metadata_indexed: bool,
    /// インデックス済みオブジェクト数
    pub metadata_count: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            current_path: S3Path::root(),
            items: Vec::new(),
            cursor: 0,
            mode: Mode::Loading,
            running: true,
            show_banner: true,
            selected: HashSet::new(),
            filter: String::new(),
            all_items: Vec::new(),
            preview_content: None,
            preview_scroll: 0,
            search_query: String::new(),
            metadata_indexed: false,
            metadata_count: 0,
        }
    }

    /// イベントを処理して新しい状態とコマンドを返す（純粋関数）
    pub fn handle_event(self, event: Event) -> (Self, Option<Command>) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::ItemsLoaded(items) => self.handle_items_loaded(items),
            Event::PreviewLoaded(content) => (
                Self {
                    preview_content: Some(content),
                    preview_scroll: 0,
                    mode: Mode::Preview,
                    ..self
                },
                None,
            ),
            Event::SearchResults(results) => (
                Self {
                    items: results,
                    cursor: 0,
                    mode: Mode::Normal,
                    ..self
                },
                None,
            ),
            Event::MetadataIndexed(count) => (
                Self {
                    metadata_indexed: true,
                    metadata_count: count,
                    ..self
                },
                None,
            ),
            Event::Error(msg) => {
                eprintln!("Error: {}", msg);
                (
                    Self {
                        mode: Mode::Normal,
                        ..self
                    },
                    None,
                )
            }
            Event::Quit => (
                Self {
                    running: false,
                    ..self
                },
                Some(Command::Quit),
            ),
        }
    }

    fn handle_key(self, key: KeyEvent) -> (Self, Option<Command>) {
        // バナー表示中は任意のキーでバナーを閉じる
        if self.show_banner {
            return (
                Self {
                    show_banner: false,
                    ..self
                },
                None,
            );
        }

        match self.mode {
            Mode::Filter => self.handle_filter_key(key),
            Mode::Preview => self.handle_preview_key(key),
            Mode::Search => self.handle_search_key(key),
            _ => self.handle_normal_key(key),
        }
    }

    fn handle_normal_key(self, key: KeyEvent) -> (Self, Option<Command>) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => (self.move_cursor_up(), None),
            KeyCode::Down | KeyCode::Char('j') => (self.move_cursor_down(), None),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.enter_item(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => self.go_back(),
            KeyCode::Char(' ') => (self.toggle_selection(), None),
            KeyCode::Char('a') => (self.toggle_select_all(), None),
            KeyCode::Char('/') => (self.enter_filter_mode(), None),
            KeyCode::Char('d') => self.start_download(),
            KeyCode::Char('?') => (self.enter_search_mode(), None),
            _ => (self, None),
        }
    }

    fn handle_filter_key(mut self, key: KeyEvent) -> (Self, Option<Command>) {
        match key.code {
            KeyCode::Enter => (self.apply_filter(), None),
            KeyCode::Esc => (self.clear_filter(), None),
            KeyCode::Backspace => {
                self.filter.pop();
                (self, None)
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                (self, None)
            }
            _ => (self, None),
        }
    }

    fn handle_preview_key(self, key: KeyEvent) -> (Self, Option<Command>) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => (
                Self {
                    mode: Mode::Normal,
                    preview_content: None,
                    preview_scroll: 0,
                    ..self
                },
                None,
            ),
            KeyCode::Down | KeyCode::Char('j') => (
                Self {
                    preview_scroll: self.preview_scroll.saturating_add(1),
                    ..self
                },
                None,
            ),
            KeyCode::Up | KeyCode::Char('k') => (
                Self {
                    preview_scroll: self.preview_scroll.saturating_sub(1),
                    ..self
                },
                None,
            ),
            KeyCode::Right | KeyCode::Char('l') => (self.next_pdf_page(), None),
            KeyCode::Left | KeyCode::Char('h') => (self.prev_pdf_page(), None),
            _ => (self, None),
        }
    }

    fn next_pdf_page(self) -> Self {
        if let Some(PreviewContent::Pdf {
            current_page,
            total_pages,
            ..
        }) = &self.preview_content
            && current_page + 1 < *total_pages
        {
            let mut content = self.preview_content.clone();
            if let Some(PreviewContent::Pdf {
                current_page: ref mut cp,
                ..
            }) = content
            {
                *cp += 1;
            }
            return Self {
                preview_content: content,
                ..self
            };
        }
        self
    }

    fn prev_pdf_page(self) -> Self {
        if let Some(PreviewContent::Pdf { current_page, .. }) = &self.preview_content
            && *current_page > 0
        {
            let mut content = self.preview_content.clone();
            if let Some(PreviewContent::Pdf {
                current_page: ref mut cp,
                ..
            }) = content
            {
                *cp -= 1;
            }
            return Self {
                preview_content: content,
                ..self
            };
        }
        self
    }

    fn handle_items_loaded(self, items: Vec<S3Item>) -> (Self, Option<Command>) {
        (
            Self {
                items,
                cursor: 0,
                mode: Mode::Normal,
                selected: HashSet::new(),
                filter: String::new(),
                all_items: Vec::new(),
                ..self
            },
            None,
        )
    }

    fn move_cursor_up(self) -> Self {
        let cursor = if self.cursor > 0 {
            self.cursor - 1
        } else {
            self.cursor
        };
        Self { cursor, ..self }
    }

    fn move_cursor_down(self) -> Self {
        let cursor = if self.cursor < self.items.len().saturating_sub(1) {
            self.cursor + 1
        } else {
            self.cursor
        };
        Self { cursor, ..self }
    }

    fn enter_item(self) -> (Self, Option<Command>) {
        let item = match self.items.get(self.cursor) {
            Some(item) => item.clone(),
            None => return (self, None),
        };
        match item {
            S3Item::Bucket { ref name } => {
                let new_path = S3Path::bucket(name);
                (
                    Self {
                        current_path: new_path.clone(),
                        mode: Mode::Loading,
                        selected: HashSet::new(),
                        ..self
                    },
                    Some(Command::LoadItems(new_path)),
                )
            }
            S3Item::Folder { ref prefix, .. } => {
                let new_path = S3Path::with_prefix(
                    self.current_path.bucket.clone().unwrap_or_default(),
                    prefix,
                );
                (
                    Self {
                        current_path: new_path.clone(),
                        mode: Mode::Loading,
                        selected: HashSet::new(),
                        ..self
                    },
                    Some(Command::LoadItems(new_path)),
                )
            }
            S3Item::File {
                ref key, ref name, ..
            } => {
                if crate::preview::text::is_previewable(name)
                    || crate::preview::image::is_image(name)
                    || crate::preview::pdf::is_pdf(name)
                {
                    let bucket = self.current_path.bucket.clone().unwrap_or_default();
                    (
                        Self {
                            mode: Mode::Loading,
                            ..self
                        },
                        Some(Command::LoadPreview {
                            bucket,
                            key: key.clone(),
                        }),
                    )
                } else {
                    (self, None)
                }
            }
        }
    }

    fn go_back(self) -> (Self, Option<Command>) {
        if let Some(parent) = self.current_path.parent() {
            (
                Self {
                    current_path: parent.clone(),
                    mode: Mode::Loading,
                    selected: HashSet::new(),
                    ..self
                },
                Some(Command::LoadItems(parent)),
            )
        } else {
            (self, None)
        }
    }

    /// 初期ロードコマンドを返す
    pub fn initial_command(&self) -> Command {
        Command::LoadItems(self.current_path.clone())
    }

    /// 選択中のアイテムを返す
    pub fn selected_item(&self) -> Option<&S3Item> {
        self.items.get(self.cursor)
    }

    fn toggle_selection(mut self) -> Self {
        if self.selected.contains(&self.cursor) {
            self.selected.remove(&self.cursor);
        } else {
            self.selected.insert(self.cursor);
        }
        self
    }

    fn toggle_select_all(mut self) -> Self {
        if self.selected.len() == self.items.len() {
            self.selected.clear();
        } else {
            self.selected = (0..self.items.len()).collect();
        }
        self
    }

    pub fn selected_items(&self) -> Vec<&S3Item> {
        self.selected
            .iter()
            .filter_map(|&i| self.items.get(i))
            .collect()
    }

    fn start_download(self) -> (Self, Option<Command>) {
        let bucket = match &self.current_path.bucket {
            Some(b) => b.clone(),
            None => return (self, None),
        };
        let key = self.selected_item().and_then(|item| match item {
            S3Item::File { key, .. } => Some(key.clone()),
            _ => None,
        });
        let key = match key {
            Some(k) => k,
            None => return (self, None),
        };
        let destination = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        (
            self,
            Some(Command::Download {
                bucket,
                key,
                destination,
            }),
        )
    }

    fn enter_filter_mode(self) -> Self {
        let all_items = if self.all_items.is_empty() {
            self.items.clone()
        } else {
            self.all_items
        };
        Self {
            mode: Mode::Filter,
            all_items,
            filter: String::new(),
            ..self
        }
    }

    fn apply_filter(self) -> Self {
        let filtered = if self.filter.is_empty() {
            self.all_items.clone()
        } else {
            let escaped = regex::escape(&self.filter);
            let pattern = escaped.replace(r"\*", ".*");
            let re = regex::Regex::new(&format!("(?i){}", pattern)).ok();
            self.all_items
                .iter()
                .filter(|item| re.as_ref().is_none_or(|r| r.is_match(item.name())))
                .cloned()
                .collect()
        };
        Self {
            items: filtered,
            cursor: 0,
            mode: Mode::Normal,
            selected: HashSet::new(),
            ..self
        }
    }

    fn clear_filter(self) -> Self {
        let items = if self.all_items.is_empty() {
            self.items
        } else {
            self.all_items.clone()
        };
        Self {
            items,
            filter: String::new(),
            cursor: 0,
            mode: Mode::Normal,
            selected: HashSet::new(),
            ..self
        }
    }

    fn enter_search_mode(self) -> Self {
        Self {
            mode: Mode::Search,
            search_query: String::new(),
            ..self
        }
    }

    fn handle_search_key(mut self, key: KeyEvent) -> (Self, Option<Command>) {
        match key.code {
            KeyCode::Enter => {
                let query = self.search_query.clone();
                (
                    Self {
                        mode: Mode::Loading,
                        ..self
                    },
                    Some(Command::ExecuteSearch(query)),
                )
            }
            KeyCode::Esc => (
                Self {
                    mode: Mode::Normal,
                    search_query: String::new(),
                    ..self
                },
                None,
            ),
            KeyCode::Backspace => {
                self.search_query.pop();
                (self, None)
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                (self, None)
            }
            _ => (self, None),
        }
    }
}
