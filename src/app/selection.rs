use crate::command::Command;
use crate::s3::S3Item;

use super::App;

impl App {
    pub(crate) fn toggle_selection(mut self) -> Self {
        if self.selected.contains(&self.cursor) {
            self.selected.remove(&self.cursor);
        } else {
            self.selected.insert(self.cursor);
        }
        self
    }

    pub(crate) fn toggle_select_all(mut self) -> Self {
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

    pub(crate) fn start_download(self) -> (Self, Vec<Command>) {
        let bucket = match &self.current_path.bucket {
            Some(b) => b.clone(),
            None => return (self, vec![]),
        };
        let item = match self.selected_item() {
            Some(item) => item.clone(),
            None => return (self, vec![]),
        };
        let download_path = super::download::default_download_path();

        match item {
            S3Item::File {
                name, key, size, ..
            } => (
                Self {
                    mode: super::Mode::DownloadConfirm,
                    download_target: Some(super::download::DownloadTarget::SingleFile {
                        name,
                        key,
                        size,
                    }),
                    download_path,
                    confirm_focus: super::download::ConfirmFocus::default(),
                    confirm_button: super::download::ConfirmButton::default(),
                    path_completions: Vec::new(),
                    completion_index: 0,
                    ..self
                },
                vec![],
            ),
            S3Item::Folder { prefix, .. } => (
                Self {
                    mode: super::Mode::Loading,
                    download_path,
                    ..self
                },
                vec![Command::ListFolderFiles { bucket, prefix }],
            ),
            _ => (self, vec![]),
        }
    }

    /// 確認ダイアログからダウンロード実行
    pub(crate) fn execute_download(self) -> (Self, Vec<Command>) {
        let bucket = match &self.current_path.bucket {
            Some(b) => b.clone(),
            None => return (self, vec![]),
        };
        let destination = super::download::expand_path(&self.download_path);

        let (keys, base_prefix) = match &self.download_target {
            Some(super::download::DownloadTarget::SingleFile { key, .. }) => {
                (vec![key.clone()], String::new())
            }
            Some(super::download::DownloadTarget::Folder { prefix, keys, .. }) => {
                (keys.clone(), prefix.clone())
            }
            None => return (self, vec![]),
        };

        (
            Self {
                mode: super::Mode::Downloading,
                download_progress: Some(super::download::DownloadProgress {
                    completed: 0,
                    total: keys.len(),
                    current_file: String::new(),
                }),
                ..self
            },
            vec![Command::StartDownload {
                bucket,
                keys,
                destination,
                base_prefix,
            }],
        )
    }

    /// Tab 補完サイクル
    pub(crate) fn cycle_path_completion(mut self) -> (Self, Vec<Command>) {
        if self.path_completions.is_empty() {
            self.path_completions = super::download::complete_path(&self.download_path);
            self.completion_index = 0;
        }

        if let Some(completion) = self.path_completions.get(self.completion_index) {
            self.download_path = completion.clone();
            self.completion_index = (self.completion_index + 1) % self.path_completions.len();
        }

        (self, vec![])
    }
}
