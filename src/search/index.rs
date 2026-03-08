use rusqlite::{Connection, params};

use crate::error::{Result, S3vError};
use crate::s3::S3Item;

pub struct MetadataIndex {
    conn: Connection,
}

impl MetadataIndex {
    pub fn new() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| S3vError::Terminal(e.to_string()))?;

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
            CREATE INDEX IF NOT EXISTS idx_extension ON objects(extension);",
        )
        .map_err(|e| S3vError::Terminal(e.to_string()))?;

        Ok(Self { conn })
    }

    pub fn insert_items(&self, items: &[S3Item]) -> Result<usize> {
        let mut count = 0;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| S3vError::Terminal(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO objects (key, name, prefix, extension, size, modified, is_folder)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|e| S3vError::Terminal(e.to_string()))?;

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
                        .map_err(|e| S3vError::Terminal(e.to_string()))?;
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
                        .map_err(|e| S3vError::Terminal(e.to_string()))?;
                        count += 1;
                    }
                    _ => {}
                }
            }
        }

        tx.commit().map_err(|e| S3vError::Terminal(e.to_string()))?;
        Ok(count)
    }

    pub fn search(&self, where_clause: &str) -> Result<Vec<S3Item>> {
        let sql = format!(
            "SELECT key, name, prefix, size, modified, is_folder FROM objects WHERE {}",
            where_clause
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| S3vError::Terminal(format!("SQL error: {}", e)))?;

        let items = stmt
            .query_map([], |row| {
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
            .map_err(|e| S3vError::Terminal(e.to_string()))?;

        let mut result = Vec::new();
        for item in items {
            result.push(item.map_err(|e| S3vError::Terminal(e.to_string()))?);
        }
        Ok(result)
    }

    pub fn count(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0))
            .unwrap_or(0)
    }
}
