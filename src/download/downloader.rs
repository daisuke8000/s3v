use std::path::Path;

use aws_sdk_s3::Client;
use tokio::io::AsyncWriteExt;

use crate::error::{Result, S3vError};

pub async fn download_file(
    client: &Client,
    bucket: &str,
    key: &str,
    destination: &Path,
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

    let file_name = key.split('/').next_back().unwrap_or(key);
    let file_path = destination.join(file_name);

    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::File::create(&file_path).await?;
    file.write_all(&body.into_bytes()).await?;

    Ok(())
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
    let relative = if base_prefix.is_empty() {
        key.split('/').next_back().unwrap_or(key)
    } else {
        key.strip_prefix(base_prefix).unwrap_or(key)
    };

    let file_path = destination.join(relative);

    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::File::create(&file_path).await?;
    file.write_all(&body.into_bytes()).await?;

    Ok(())
}
