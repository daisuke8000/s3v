use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::s3::S3Item;

/// アプリケーションイベント
#[derive(Debug, Clone)]
pub enum Event {
    /// キー入力
    Key(KeyEvent),
    /// アイテム一覧の読み込み完了
    ItemsLoaded(Vec<S3Item>),
    /// エラー発生
    Error(String),
    /// 終了要求
    Quit,
}

impl Event {
    /// キーイベントから Event を生成
    pub fn from_key(key: KeyEvent) -> Self {
        // Ctrl+C は終了
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Event::Quit;
        }

        // 'q' は終了
        if key.code == KeyCode::Char('q') {
            return Event::Quit;
        }

        Event::Key(key)
    }
}
