use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, params};

use crate::error::{Result, S3vError};
use crate::s3::S3Item;

pub struct MetadataIndex {
    conn: Mutex<Connection>,
}

impl MetadataIndex {
    /// バケット名からキャッシュパスを決定して開く
    pub fn open(bucket: &str) -> Result<Self> {
        let path = cache_path(bucket);
        Self::open_path(&path)
    }

    /// 指定パスで開く（テスト用にも使用）
    pub fn open_path(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .or_else(|_| Connection::open_in_memory())
            .map_err(|e| S3vError::Search(e.to_string()))?;

        let index = Self {
            conn: Mutex::new(conn),
        };
        index.create_tables()?;
        Ok(index)
    }

    /// インメモリで開く（後方互換 + フォールバック）
    pub fn new() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| S3vError::Search(e.to_string()))?;
        let index = Self {
            conn: Mutex::new(conn),
        };
        index.create_tables()?;
        Ok(index)
    }

    fn create_tables(&self) -> Result<()> {
        let conn = self.lock()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS objects (
                key       TEXT PRIMARY KEY,
                name      TEXT NOT NULL,
                prefix    TEXT NOT NULL,
                extension TEXT,
                size      INTEGER NOT NULL,
                modified  TEXT,
                is_folder INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_name ON objects(name);
            CREATE INDEX IF NOT EXISTS idx_modified ON objects(modified);
            CREATE INDEX IF NOT EXISTS idx_extension ON objects(extension);
            CREATE TABLE IF NOT EXISTS indexed_prefixes (
                prefix     TEXT PRIMARY KEY,
                indexed_at TEXT NOT NULL
            );",
        )
        .map_err(|e| S3vError::Search(e.to_string()))?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| S3vError::Search(e.to_string()))
    }

    pub fn insert_items(&self, items: &[S3Item]) -> Result<usize> {
        let mut count = 0;
        let conn = self.lock()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| S3vError::Search(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO objects (key, name, prefix, extension, size, modified, is_folder)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|e| S3vError::Search(e.to_string()))?;

            for item in items {
                match item {
                    S3Item::File {
                        name,
                        key,
                        size,
                        last_modified,
                    } => {
                        let prefix = key.rfind('/').map(|i| &key[..=i]).unwrap_or("");
                        let extension = name.rfind('.').map(|i| &name[i..]);
                        stmt.execute(params![
                            key,
                            name,
                            prefix,
                            extension,
                            *size as i64,
                            last_modified.as_deref(),
                            false
                        ])
                        .map_err(|e| S3vError::Search(e.to_string()))?;
                        count += 1;
                    }
                    S3Item::Folder { name, prefix } => {
                        stmt.execute(params![
                            prefix,
                            name,
                            prefix,
                            Option::<String>::None,
                            0i64,
                            Option::<String>::None,
                            true
                        ])
                        .map_err(|e| S3vError::Search(e.to_string()))?;
                        count += 1;
                    }
                    _ => {}
                }
            }
        }

        tx.commit().map_err(|e| S3vError::Search(e.to_string()))?;
        Ok(count)
    }

    /// prefix がインデックス済みか確認（親 prefix も含めて）
    pub fn is_prefix_covered(&self, prefix: &str) -> Result<bool> {
        let conn = self.lock()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_prefixes WHERE prefix != '' AND ?1 LIKE prefix || '%'",
                params![prefix],
                |row| row.get(0),
            )
            .map_err(|e| S3vError::Search(e.to_string()))?;
        Ok(count > 0)
    }

    /// prefix をインデックス済みとして記録
    pub fn mark_prefix_indexed(&self, prefix: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO indexed_prefixes (prefix, indexed_at) VALUES (?1, datetime('now'))",
            params![prefix],
        )
        .map_err(|e| S3vError::Search(e.to_string()))?;
        Ok(())
    }

    /// 指定 prefix 配下を検索
    pub fn search(&self, prefix: &str, where_clause: &str) -> Result<Vec<S3Item>> {
        validate_where_clause(where_clause)?;

        let sql = format!(
            "SELECT key, name, prefix, size, modified, is_folder FROM objects WHERE key LIKE ?1 AND ({}) LIMIT 1000",
            where_clause
        );

        let conn = self.lock()?;
        let like_pattern = format!("{}%", prefix);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|_| S3vError::Search("Invalid SQL query".to_string()))?;

        let items = stmt
            .query_map(params![like_pattern], |row| {
                let is_folder: bool = row.get(5)?;
                let key: String = row.get(0)?;
                let name: String = row.get(1)?;

                if is_folder {
                    Ok(S3Item::Folder { name, prefix: key })
                } else {
                    let size: i64 = row.get(3)?;
                    let modified: Option<String> = row.get(4)?;
                    Ok(S3Item::File {
                        name,
                        key,
                        size: size as u64,
                        last_modified: modified,
                    })
                }
            })
            .map_err(|e| S3vError::Search(e.to_string()))?;

        let mut result = Vec::new();
        for item in items {
            result.push(item.map_err(|e| S3vError::Search(e.to_string()))?);
        }
        Ok(result)
    }
}

fn cache_path(bucket: &str) -> PathBuf {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("s3v");
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        eprintln!("Warning: cannot create cache directory: {}", e);
    }
    // バケット名のサニタイズ（パストラバーサル防止）
    let safe_name: String = bucket
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    cache_dir.join(format!("{}.db", safe_name))
}

/// 許可するカラム名
const ALLOWED_COLUMNS: &[&str] = &[
    "name",
    "key",
    "prefix",
    "size",
    "modified",
    "extension",
    "is_folder",
];

/// 許可する SQL トークン（カラム名・演算子・リテラル・論理演算子のみ）
fn validate_where_clause(clause: &str) -> Result<()> {
    if clause.trim().is_empty() {
        return Err(S3vError::Search("Empty query".to_string()));
    }

    let lower = clause.to_lowercase();

    // 危険な文字・キーワードを拒否
    let denied_chars = [";", "--", "/*", "*/", "||"];
    for ch in &denied_chars {
        if lower.contains(ch) {
            return Err(S3vError::Search(format!(
                "Invalid query: '{}' is not allowed",
                ch
            )));
        }
    }

    let denied_keywords = [
        "attach",
        "pragma",
        "drop",
        "delete",
        "insert",
        "update",
        "create",
        "alter",
        "detach",
        "reindex",
        "vacuum",
        "select",
        "union",
        "into",
        "exec",
        "load_extension",
    ];
    // 単語境界でキーワードを検出（"updated" のような列名を誤検出しない）
    for keyword in &denied_keywords {
        let pattern = format!(r"\b{}\b", keyword);
        if regex::Regex::new(&pattern)
            .ok()
            .is_some_and(|re| re.is_match(&lower))
        {
            return Err(S3vError::Search(format!(
                "Invalid query: '{}' is not allowed",
                keyword
            )));
        }
    }

    // 文字列リテラルを除去してから識別子を検証
    // 'abc' のようなリテラルを空に置換し、残った識別子のみチェック
    let stripped = regex::Regex::new(r"'[^']*'")
        .map_err(|e| S3vError::Search(e.to_string()))?
        .replace_all(&lower, " ");

    // カラム参照の検証: WHERE 句内のカラム名が許可リストに含まれるか確認
    let word_re =
        regex::Regex::new(r"\b([a-z_]+)\b").map_err(|e| S3vError::Search(e.to_string()))?;
    let allowed_words: &[&str] = &[
        "and", "or", "not", "like", "glob", "between", "in", "is", "null", "true", "false",
    ];
    for cap in word_re.captures_iter(&stripped) {
        let word = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        // 数値リテラル、許可カラム名、SQL キーワードのいずれかであればOK
        if word.chars().all(|c| c.is_ascii_digit())
            || ALLOWED_COLUMNS.contains(&word)
            || allowed_words.contains(&word)
        {
            continue;
        }
        return Err(S3vError::Search(format!(
            "Invalid query: unknown identifier '{}'",
            word
        )));
    }

    Ok(())
}
