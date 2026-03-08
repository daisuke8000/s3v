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
        let download_path = super::download::default_download_path();

        // 複数選択がある場合はすべての選択アイテムを対象にする
        if !self.selected.is_empty() {
            let selected_items: Vec<S3Item> = self.selected_items().into_iter().cloned().collect();

            // ファイルの key と合計サイズを収集
            let mut keys = Vec::new();
            let mut total_size = 0u64;
            for item in &selected_items {
                if let S3Item::File { key, size, .. } = item {
                    keys.push(key.clone());
                    total_size += size;
                }
            }

            if keys.is_empty() {
                return (self, vec![]);
            }

            let base_prefix = self.current_path.prefix.clone();

            return (
                Self {
                    mode: super::Mode::DownloadConfirm,
                    download_target: Some(super::download::DownloadTarget::MultipleFiles {
                        keys,
                        total_size,
                        base_prefix,
                    }),
                    download_path,
                    confirm_focus: super::download::ConfirmFocus::default(),
                    confirm_button: super::download::ConfirmButton::default(),
                    path_completions: Vec::new(),
                    completion_index: 0,
                    ..self
                },
                vec![],
            );
        }

        // 単一選択（カーソル位置）
        let item = match self.selected_item() {
            Some(item) => item.clone(),
            None => return (self, vec![]),
        };

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

        match &self.download_target {
            Some(super::download::DownloadTarget::SingleFile { key, .. }) => {
                // 単一ファイル: key のディレクトリ部分を base_prefix に設定
                // e.g. "folder/sub/test.json" → base_prefix="folder/sub/"
                let bp = key.rfind('/').map(|i| &key[..=i]).unwrap_or("").to_string();
                let keys = vec![key.clone()];
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
                        base_prefix: bp,
                    }],
                )
            }
            Some(super::download::DownloadTarget::MultipleFiles {
                keys,
                total_size,
                base_prefix,
            }) => {
                let keys = keys.clone();
                let base_prefix = base_prefix.clone();
                let total_size = *total_size;
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
                    vec![Command::StartZipDownload {
                        bucket,
                        keys,
                        destination,
                        base_prefix,
                        archive_name: "s3v-download".to_string(),
                        total_size,
                    }],
                )
            }
            Some(super::download::DownloadTarget::Folder {
                prefix,
                keys,
                total_size,
                ..
            }) => {
                // フォルダ名を archive 名に使用（末尾 / を除去）
                let folder_name = prefix
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("folder")
                    .to_string();
                // prefix の親を base_prefix にする
                // e.g. prefix="photos/vacation/" → parent="photos/"
                let base_prefix = prefix
                    .trim_end_matches('/')
                    .rfind('/')
                    .map(|i| &prefix[..=i])
                    .unwrap_or("")
                    .to_string();
                let keys = keys.clone();
                let total_size = *total_size;
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
                    vec![Command::StartZipDownload {
                        bucket,
                        keys,
                        destination,
                        base_prefix,
                        archive_name: folder_name,
                        total_size,
                    }],
                )
            }
            None => (self, vec![]),
        }
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
