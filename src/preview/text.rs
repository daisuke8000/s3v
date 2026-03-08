const PREVIEWABLE_EXTENSIONS: &[&str] = &[
    "txt",
    "json",
    "md",
    "csv",
    "log",
    "yaml",
    "yml",
    "toml",
    "xml",
    "html",
    "css",
    "js",
    "ts",
    "rs",
    "py",
    "go",
    "sh",
    "sql",
    "conf",
    "ini",
    "env",
    "dockerfile",
];

pub fn is_previewable(name: &str) -> bool {
    super::has_extension(name, PREVIEWABLE_EXTENSIONS)
}

/// バイト列から安全な UTF-8 境界位置を返す
///
/// マルチバイト文字がチャンク境界で分断される場合、
/// 完全な文字として解釈可能な末尾位置を返す。
/// 残りのバイトは次のチャンクと結合して処理する。
pub fn find_valid_utf8_boundary(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => e.valid_up_to(),
    }
}

pub fn format_preview(content: &str, key: &str) -> String {
    if key.ends_with(".json") {
        serde_json::from_str::<serde_json::Value>(content)
            .and_then(|v| serde_json::to_string_pretty(&v))
            .unwrap_or_else(|_| content.to_string())
    } else {
        content.to_string()
    }
}
