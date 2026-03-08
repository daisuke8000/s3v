use crate::command::Command;
use crate::preview::PreviewContent;
use crate::s3::S3Item;

use super::App;

impl App {
    /// カーソル位置のアイテムに基づいて自動プレビューコマンドを生成
    pub fn build_auto_preview_commands(&mut self) -> Vec<Command> {
        let mut cmds = Vec::new();

        let bucket = match &self.current_path.bucket {
            Some(b) => b.clone(),
            None => return cmds,
        };

        let item = match self.items.get(self.cursor) {
            Some(item) => item.clone(),
            None => {
                // カーソル位置にアイテムがない場合はプレビューをクリア
                self.preview_content = None;
                self.folder_preview_items.clear();
                return cmds;
            }
        };

        match &item {
            S3Item::Folder { prefix, .. } => {
                // フォルダ: フォルダ内容をプレビュー
                self.preview_content = None;
                let debounce_key = format!("folder:{}", prefix);
                self.pending_preview_key = Some(debounce_key.clone());
                cmds.push(Command::RequestPreview {
                    bucket: bucket.clone(),
                    key: prefix.clone(),
                    debounce_key,
                });
            }
            S3Item::File { key, name, .. } => {
                if !crate::preview::is_previewable_file(name) {
                    self.preview_content = None;
                    self.folder_preview_items.clear();
                    return cmds;
                }

                self.folder_preview_items.clear();

                // キャッシュヒットなら即座にセット
                if let Some(cached) = self.preview_cache.get(key) {
                    self.preview_content = Some(PreviewContent::Text(cached.clone()));
                    self.preview_scroll = 0;
                    // コマンド不要
                } else {
                    // キャッシュミス: デバウンス付きプレビュー要求
                    let debounce_key = format!("file:{}", key);
                    self.pending_preview_key = Some(debounce_key.clone());
                    cmds.push(Command::RequestPreview {
                        bucket: bucket.clone(),
                        key: key.clone(),
                        debounce_key,
                    });
                }

                // プリフェッチ: カーソル前後2アイテムの未キャッシュファイルを先読み
                cmds.extend(self.build_prefetch_commands(&bucket));
            }
            S3Item::Bucket { name } => {
                // バケット一覧ではフォルダプレビューとして内容を表示
                self.preview_content = None;
                let debounce_key = format!("bucket:{}", name);
                self.pending_preview_key = Some(debounce_key.clone());
                cmds.push(Command::RequestPreview {
                    bucket: name.clone(),
                    key: String::new(),
                    debounce_key,
                });
            }
        }

        cmds
    }

    /// カーソル前後2アイテムのプリフェッチコマンドを生成
    fn build_prefetch_commands(&self, bucket: &str) -> Vec<Command> {
        let mut cmds = Vec::new();
        let cursor = self.cursor;
        let len = self.items.len();

        let start = cursor.saturating_sub(2);
        let end = (cursor + 3).min(len);

        for i in start..end {
            if i == cursor {
                continue;
            }
            if let Some(S3Item::File { key, name, .. }) = self.items.get(i)
                && crate::preview::text::is_previewable(name)
                && !self.preview_cache.contains_key(key)
            {
                cmds.push(Command::PrefetchPreview {
                    bucket: bucket.to_string(),
                    key: key.clone(),
                });
            }
        }

        cmds
    }
}
