pub mod download;
mod filter;
mod key_handlers;
mod navigation;
mod preview_logic;
mod selection;

use std::collections::{HashMap, HashSet};

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
    /// プレビューペインにフォーカス（jk でスクロール）
    PreviewFocus,
    Search,
    /// ダウンロード確認ダイアログ
    DownloadConfirm,
    /// ダウンロード進捗表示
    Downloading,
}

/// バナー表示状態
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BannerState {
    /// バナーのみ表示（起動直後）
    #[default]
    Splash,
    /// バナー（上部コンパクト） + 操作画面
    Active,
}

/// プレビューキャッシュの最大エントリ数
const MAX_PREVIEW_CACHE: usize = 50;

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
    /// バナー表示状態
    pub banner_state: BannerState,
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
    /// エラーメッセージ（UI に表示）
    pub error_message: Option<String>,
    /// 成功メッセージ（UI に表示）
    pub status_message: Option<String>,
    /// 親ペイン: 親ディレクトリのアイテム一覧
    pub parent_items: Vec<S3Item>,
    /// 親ペイン: 現在パスに対応するハイライト位置
    pub parent_cursor: usize,
    /// フォルダプレビュー: カーソル上のフォルダの子アイテム一覧
    pub folder_preview_items: Vec<S3Item>,
    /// 自動プレビューの対象キー (デバウンスの一致判定用)
    pub pending_preview_key: Option<String>,
    /// テキストプレビューキャッシュ (S3 key → formatted text)
    pub preview_cache: HashMap<String, String>,
    /// DL確認: 保存先パス
    pub download_path: String,
    /// DL確認: ダイアログ内フォーカス
    pub confirm_focus: download::ConfirmFocus,
    /// DL確認: ボタン選択
    pub confirm_button: download::ConfirmButton,
    /// DL確認: Tab 補完候補
    pub path_completions: Vec<String>,
    /// DL確認: 補完候補のインデックス
    pub completion_index: usize,
    /// DL確認: 対象情報
    pub download_target: Option<download::DownloadTarget>,
    /// DL進捗
    pub download_progress: Option<download::DownloadProgress>,
}

/// テキスト入力フィールドの共通キー処理
pub(crate) fn handle_text_input(field: &mut String, key: crossterm::event::KeyEvent) -> bool {
    match key.code {
        crossterm::event::KeyCode::Backspace => {
            field.pop();
            true
        }
        crossterm::event::KeyCode::Char(c) => {
            field.push(c);
            true
        }
        _ => false,
    }
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
            banner_state: BannerState::Splash,
            selected: HashSet::new(),
            filter: String::new(),
            all_items: Vec::new(),
            preview_content: None,
            preview_scroll: 0,
            search_query: String::new(),
            metadata_indexed: false,
            metadata_count: 0,
            error_message: None,
            status_message: None,
            parent_items: Vec::new(),
            parent_cursor: 0,
            folder_preview_items: Vec::new(),
            pending_preview_key: None,
            preview_cache: HashMap::new(),
            download_path: String::new(),
            confirm_focus: download::ConfirmFocus::default(),
            confirm_button: download::ConfirmButton::default(),
            path_completions: Vec::new(),
            completion_index: 0,
            download_target: None,
            download_progress: None,
        }
    }

    /// イベントを処理して新しい状態とコマンドを返す（純粋関数）
    pub fn handle_event(self, event: Event) -> (Self, Vec<Command>) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::ItemsLoaded(items) => self.handle_items_loaded(items),
            Event::PreviewLoaded(content) => {
                // Enter キー経由（Loading）なら PreviewFocus、自動プレビューなら現モード維持
                let mode = if self.mode == Mode::Loading {
                    Mode::PreviewFocus
                } else {
                    self.mode.clone()
                };
                (
                    Self {
                        preview_content: Some(content),
                        preview_scroll: 0,
                        mode,
                        ..self
                    },
                    vec![],
                )
            }
            Event::SearchResults(results) => (
                Self {
                    items: results,
                    cursor: 0,
                    mode: Mode::Normal,
                    ..self
                },
                vec![],
            ),
            Event::MetadataIndexed(count) => (
                Self {
                    metadata_indexed: true,
                    metadata_count: count,
                    ..self
                },
                vec![],
            ),
            Event::PreviewChunk(chunk) => {
                let (partial_text, key) = match self.preview_content {
                    Some(PreviewContent::StreamingText {
                        mut partial_text,
                        key,
                    }) => {
                        partial_text.push_str(&chunk);
                        (partial_text, key)
                    }
                    _ => (chunk, String::new()),
                };
                (
                    Self {
                        preview_content: Some(PreviewContent::StreamingText { partial_text, key }),
                        ..self
                    },
                    vec![],
                )
            }
            Event::PreviewStreamComplete(formatted) => {
                let content = match formatted {
                    Some(text) => text,
                    None => match &self.preview_content {
                        Some(PreviewContent::StreamingText { partial_text, .. }) => {
                            partial_text.clone()
                        }
                        _ => String::new(),
                    },
                };
                // Enter キー経由（Loading）なら PreviewFocus、自動プレビューなら現モード維持
                let mode = if self.mode == Mode::Loading {
                    Mode::PreviewFocus
                } else {
                    self.mode.clone()
                };
                (
                    Self {
                        preview_content: Some(PreviewContent::Text(content)),
                        preview_scroll: 0,
                        mode,
                        ..self
                    },
                    vec![],
                )
            }
            Event::PreviewProgress { received, total } => (
                Self {
                    preview_content: Some(PreviewContent::Downloading { received, total }),
                    ..self
                },
                vec![],
            ),
            Event::PreviewImageReady => {
                // Enter キー経由（Loading）なら PreviewFocus、自動プレビューなら現モード維持
                let mode = if self.mode == Mode::Loading {
                    Mode::PreviewFocus
                } else {
                    self.mode.clone()
                };
                (
                    Self {
                        preview_content: Some(PreviewContent::Image),
                        mode,
                        ..self
                    },
                    vec![],
                )
            }
            Event::PdfDataReady => (self, vec![]),
            Event::ParentItemsLoaded(items) => {
                let parent_cursor = self.find_parent_cursor(&items);
                (
                    Self {
                        parent_items: items,
                        parent_cursor,
                        ..self
                    },
                    vec![],
                )
            }
            Event::FolderPreviewLoaded(items) => (
                Self {
                    folder_preview_items: items,
                    ..self
                },
                vec![],
            ),
            Event::DebounceTimeout { .. } => {
                // デバウンス処理は main.rs 側で実行
                (self, vec![])
            }
            Event::PrefetchComplete { key, content } => {
                let mut new_self = self;
                new_self.cache_preview(key, content);
                (new_self, vec![])
            }
            Event::FolderFilesListed { files, total_size } => {
                let keys: Vec<String> = files
                    .iter()
                    .filter_map(|f| match f {
                        S3Item::File { key, .. } => Some(key.clone()),
                        _ => None,
                    })
                    .collect();
                let file_count = keys.len();
                // フォルダ名を download_path から推測（prefix の最後のセグメント）
                let folder_name = keys
                    .first()
                    .and_then(|k| {
                        let parts: Vec<&str> = k.rsplitn(2, '/').collect();
                        if parts.len() > 1 {
                            let prefix_part = parts[1];
                            prefix_part
                                .trim_end_matches('/')
                                .rsplit('/')
                                .next()
                                .map(|s| format!("{}/", s))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                let prefix = keys
                    .first()
                    .and_then(|k| {
                        let parts: Vec<&str> = k.rsplitn(2, '/').collect();
                        if parts.len() > 1 {
                            Some(format!("{}/", parts[1]))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                (
                    Self {
                        mode: Mode::DownloadConfirm,
                        download_target: Some(download::DownloadTarget::Folder {
                            name: folder_name,
                            prefix,
                            file_count,
                            total_size,
                            keys,
                        }),
                        confirm_focus: download::ConfirmFocus::default(),
                        confirm_button: download::ConfirmButton::default(),
                        ..self
                    },
                    vec![],
                )
            }
            Event::DownloadFileComplete {
                completed,
                total,
                current_file,
            } => (
                Self {
                    download_progress: Some(download::DownloadProgress {
                        completed,
                        total,
                        current_file,
                    }),
                    ..self
                },
                vec![],
            ),
            Event::DownloadAllComplete { count } => (
                Self {
                    mode: Mode::Normal,
                    download_target: None,
                    download_progress: None,
                    status_message: Some(format!("{} files downloaded", count)),
                    ..self
                },
                vec![],
            ),
            Event::Error(msg) => (
                Self {
                    mode: Mode::Normal,
                    error_message: Some(msg),
                    ..self
                },
                vec![],
            ),
            Event::Quit => (
                Self {
                    running: false,
                    ..self
                },
                vec![Command::Quit],
            ),
        }
    }

    fn handle_items_loaded(self, items: Vec<S3Item>) -> (Self, Vec<Command>) {
        (
            Self {
                items,
                cursor: 0,
                mode: Mode::Normal,
                banner_state: BannerState::Active,
                selected: HashSet::new(),
                filter: String::new(),
                all_items: Vec::new(),
                folder_preview_items: Vec::new(),
                preview_content: None,
                ..self
            },
            vec![],
        )
    }

    /// 選択中のアイテムを返す
    pub fn selected_item(&self) -> Option<&S3Item> {
        self.items.get(self.cursor)
    }

    /// 親ペイン内で現在パスに対応する位置を算出
    fn find_parent_cursor(&self, parent_items: &[S3Item]) -> usize {
        let current_name = if !self.current_path.prefix.is_empty() {
            let trimmed = self.current_path.prefix.trim_end_matches('/');
            trimmed
                .rsplit('/')
                .next()
                .map(|s| format!("{}/", s))
                .unwrap_or_default()
        } else {
            self.current_path.bucket.clone().unwrap_or_default()
        };

        parent_items
            .iter()
            .position(|item| item.name() == current_name)
            .unwrap_or(0)
    }

    /// プレビューキャッシュに格納
    pub(crate) fn cache_preview(&mut self, key: String, content: String) {
        if self.preview_cache.len() >= MAX_PREVIEW_CACHE {
            self.preview_cache.clear();
        }
        self.preview_cache.insert(key, content);
    }
}
