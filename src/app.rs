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
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => (self.move_cursor_up(), None),
            KeyCode::Down | KeyCode::Char('j') => (self.move_cursor_down(), None),
            KeyCode::Enter => self.enter_item(),
            KeyCode::Esc => self.go_back(),
            _ => (self, None),
        }
    }

    fn handle_items_loaded(self, items: Vec<S3Item>) -> (Self, Option<Command>) {
        (
            Self {
                items,
                cursor: 0,
                mode: Mode::Normal,
                show_banner: false,
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
}
