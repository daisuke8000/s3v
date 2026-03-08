use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent};

use crate::command::Command;
use crate::event::Event;
use crate::s3::{S3Item, S3Path};

/// アプリケーションモード
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Loading,
    Filter,
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
        }
    }

    /// イベントを処理して新しい状態とコマンドを返す（純粋関数）
    pub fn handle_event(self, event: Event) -> (Self, Option<Command>) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::ItemsLoaded(items) => self.handle_items_loaded(items),
            Event::Error(msg) => {
                eprintln!("Error: {}", msg);
                (self, None)
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
        if let Some(item) = self.items.get(self.cursor) {
            match item {
                S3Item::Bucket { name } => {
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
                S3Item::Folder { prefix, .. } => {
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
                S3Item::File { .. } => {
                    // Phase 1 ではファイルプレビューは未実装
                    (self, None)
                }
            }
        } else {
            (self, None)
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
                .filter(|item| re.as_ref().map_or(true, |r| r.is_match(item.name())))
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
}
