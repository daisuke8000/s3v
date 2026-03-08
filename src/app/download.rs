use std::path::PathBuf;

/// 確認ダイアログ内のフォーカス位置
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConfirmFocus {
    /// 保存先パス（テキスト入力モード）
    Path,
    /// ボタン行（開始 / キャンセル）
    #[default]
    Buttons,
}

/// ボタン行の選択状態
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConfirmButton {
    #[default]
    Start,
    Cancel,
}

/// ダウンロード対象の情報
#[derive(Debug, Clone)]
pub enum DownloadTarget {
    SingleFile {
        name: String,
        key: String,
        size: u64,
    },
    Folder {
        name: String,
        prefix: String,
        file_count: usize,
        total_size: u64,
        keys: Vec<String>,
    },
}

/// ダウンロード進捗
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub completed: usize,
    pub total: usize,
    pub current_file: String,
}

/// Tab 補完でディレクトリ候補を取得
pub fn complete_path(input: &str) -> Vec<String> {
    let expanded = shellexpand::tilde(input);
    let path = PathBuf::from(expanded.as_ref());

    let (dir, prefix) = if path.is_dir() {
        (path.clone(), String::new())
    } else {
        let dir = path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let prefix = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        (dir, prefix)
    };

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut results: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .filter_map(|e| {
            let full = e.path();
            let display = full.to_str()?.to_string();
            let home = dirs::home_dir().unwrap_or_default();
            let home_str = home.to_str().unwrap_or("");
            if !home_str.is_empty() && display.starts_with(home_str) {
                Some(format!("~{}/", &display[home_str.len()..]))
            } else {
                Some(format!("{}/", display))
            }
        })
        .collect();
    results.sort();
    results
}

/// デフォルトのダウンロードパスを取得（~/Downloads/ 形式）
pub fn default_download_path() -> String {
    let download_dir = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
    let home = dirs::home_dir().unwrap_or_default();
    let home_str = home.to_string_lossy().to_string();
    let dl_str = download_dir.to_string_lossy().to_string();

    if !home_str.is_empty() && dl_str.starts_with(&home_str) {
        format!("~{}/", &dl_str[home_str.len()..])
    } else {
        format!("{}/", dl_str)
    }
}

/// ~ を展開して PathBuf を返す
pub fn expand_path(input: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(input).as_ref())
}
