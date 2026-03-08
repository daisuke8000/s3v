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

pub fn format_preview(content: &str, key: &str) -> String {
    if key.ends_with(".json") {
        serde_json::from_str::<serde_json::Value>(content)
            .and_then(|v| serde_json::to_string_pretty(&v))
            .unwrap_or_else(|_| content.to_string())
    } else {
        content.to_string()
    }
}
