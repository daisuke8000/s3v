use std::collections::HashSet;

use crate::command::Command;
use crate::s3::{S3Item, S3Path};

use super::{App, Mode};

impl App {
    pub(crate) fn move_cursor_up(self) -> (Self, Vec<Command>) {
        if self.cursor == 0 {
            return (self, vec![]);
        }
        let cursor = self.cursor - 1;
        let mut new_self = Self { cursor, ..self };
        let cmds = new_self.build_auto_preview_commands();
        (new_self, cmds)
    }

    pub(crate) fn move_cursor_down(self) -> (Self, Vec<Command>) {
        let max = self.items.len().saturating_sub(1);
        if self.cursor >= max {
            return (self, vec![]);
        }
        let cursor = self.cursor + 1;
        let mut new_self = Self { cursor, ..self };
        let cmds = new_self.build_auto_preview_commands();
        (new_self, cmds)
    }

    /// 指定パスへナビゲートし、Loading 状態に遷移する共通ヘルパー
    fn navigate_to(self, path: S3Path) -> (Self, Vec<Command>) {
        let mut cmds = vec![Command::LoadItems(path.clone())];

        // 親ペイン用: 遷移先パスの親アイテムを取得
        if let Some(parent_path) = path.parent() {
            cmds.push(Command::LoadParentItems(parent_path));
        }

        (
            Self {
                current_path: path,
                mode: Mode::Loading,
                selected: HashSet::new(),
                ..self
            },
            cmds,
        )
    }

    pub(crate) fn enter_item(self) -> (Self, Vec<Command>) {
        let item = match self.items.get(self.cursor) {
            Some(item) => item.clone(),
            None => return (self, vec![]),
        };
        match item {
            S3Item::Bucket { ref name } => self.navigate_to(S3Path::bucket(name)),
            S3Item::Folder { ref prefix, .. } => {
                let bucket = self.current_path.bucket.clone().unwrap_or_default();
                self.navigate_to(S3Path::with_prefix(bucket, prefix))
            }
            S3Item::File {
                ref key, ref name, ..
            } => {
                if crate::preview::is_previewable_file(name) {
                    let bucket = self.current_path.bucket.clone().unwrap_or_default();
                    (
                        Self {
                            mode: Mode::Loading,
                            ..self
                        },
                        vec![Command::LoadPreview {
                            bucket,
                            key: key.clone(),
                        }],
                    )
                } else {
                    (self, vec![])
                }
            }
        }
    }

    pub(crate) fn go_back(self) -> (Self, Vec<Command>) {
        if let Some(parent) = self.current_path.parent() {
            self.navigate_to(parent)
        } else {
            (self, vec![])
        }
    }
}
