use std::path::{Component, Path, PathBuf};

use aws_sdk_s3::Client;
use tokio::io::AsyncWriteExt;

use crate::error::{Result, S3vError};

/// パスを正規化して `.` や `..` を解決する（ファイルシステムにアクセスせずに）
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// 結合後のパスが destination 配下であることを検証（パストラバーサル防止）
fn validate_download_path(destination: &Path, relative: &str) -> Result<PathBuf> {
    let file_path = destination.join(relative);
    let normalized = normalize_path(&file_path);
    let normalized_dest = normalize_path(destination);
    if !normalized.starts_with(&normalized_dest) {
        return Err(S3vError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Path traversal detected in S3 key",
        )));
    }
    Ok(unique_path(&file_path))
}

/// フォルダ構造を保持したダウンロード
/// base_prefix を key から除去した相対パスでローカルに保存
pub async fn download_file_with_structure(
    client: &Client,
    bucket: &str,
    key: &str,
    destination: &Path,
    base_prefix: &str,
) -> Result<()> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| S3vError::AwsSdk(e.to_string()))?;

    let body = resp
        .body
        .collect()
        .await
        .map_err(|e| S3vError::AwsSdk(e.to_string()))?;

    // base_prefix を除去して相対パスを得る
    let relative = key.strip_prefix(base_prefix).unwrap_or(key);

    let file_path = validate_download_path(destination, relative)?;

    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::File::create(&file_path).await?;
    file.write_all(&body.into_bytes()).await?;

    Ok(())
}

/// 同名ファイルが存在する場合に (1), (2) ... を付与したパスを返す
fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str());
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 1..1000 {
        let new_name = match ext {
            Some(e) => format!("{} ({}).{}", stem, i, e),
            None => format!("{} ({})", stem, i),
        };
        let candidate = parent.join(new_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    // フォールバック（まず起こらない）
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_unique_path_no_conflict() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        assert_eq!(unique_path(&path), path);
    }

    #[test]
    fn test_unique_path_with_conflict() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "existing").unwrap();

        let result = unique_path(&path);
        assert_eq!(result, dir.path().join("test (1).txt"));
    }

    #[test]
    fn test_unique_path_multiple_conflicts() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.json");
        fs::write(&path, "v1").unwrap();
        fs::write(dir.path().join("data (1).json"), "v2").unwrap();

        let result = unique_path(&path);
        assert_eq!(result, dir.path().join("data (2).json"));
    }

    #[test]
    fn test_unique_path_no_extension() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("README");
        fs::write(&path, "existing").unwrap();

        let result = unique_path(&path);
        assert_eq!(result, dir.path().join("README (1)"));
    }
}
