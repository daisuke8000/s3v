use aws_sdk_s3::Client;

use crate::error::{Result, S3vError};
use crate::s3::{S3Item, S3Path};

pub struct S3Client {
    client: Client,
    region: String,
}

impl S3Client {
    pub fn new(client: Client, region: String) -> Self {
        Self { client, region }
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }

    pub async fn list_buckets(&self) -> Result<Vec<S3Item>> {
        let resp = self
            .client
            .list_buckets()
            .send()
            .await
            .map_err(|e| S3vError::AwsSdk(e.to_string()))?;

        let buckets = resp
            .buckets()
            .iter()
            .filter_map(|b| {
                b.name().map(|name| S3Item::Bucket {
                    name: name.to_string(),
                })
            })
            .collect();

        Ok(buckets)
    }

    /// 指定パスのオブジェクト一覧を取得する。
    /// NOTE: ページネーション未対応（最大 1000 件）。Phase 4 で対応予定。
    pub async fn list_objects(&self, path: &S3Path) -> Result<Vec<S3Item>> {
        let bucket = path
            .bucket
            .as_ref()
            .ok_or_else(|| S3vError::AwsSdk("Bucket name required".to_string()))?;

        let mut items = Vec::new();

        let resp = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(&path.prefix)
            .delimiter("/")
            .send()
            .await
            .map_err(|e| S3vError::AwsSdk(e.to_string()))?;

        // Common prefixes (folders)
        for prefix in resp.common_prefixes() {
            if let Some(p) = prefix.prefix() {
                let name = p.strip_prefix(&path.prefix).unwrap_or(p).to_string();
                items.push(S3Item::Folder {
                    name: name.clone(),
                    prefix: p.to_string(),
                });
            }
        }

        // Objects (files)
        for obj in resp.contents() {
            if let Some(key) = obj.key() {
                // Skip the prefix itself if it appears as an object
                if key == path.prefix {
                    continue;
                }

                let name = key.strip_prefix(&path.prefix).unwrap_or(key).to_string();

                // Skip folder markers
                if name.is_empty() || name.ends_with('/') {
                    continue;
                }

                let last_modified = obj.last_modified().map(|dt| {
                    dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                        .unwrap_or_default()
                });

                items.push(S3Item::File {
                    name,
                    key: key.to_string(),
                    size: obj.size().unwrap_or(0) as u64,
                    last_modified,
                });
            }
        }

        Ok(items)
    }

    pub async fn list(&self, path: &S3Path) -> Result<Vec<S3Item>> {
        if path.is_root() {
            self.list_buckets().await
        } else {
            self.list_objects(path).await
        }
    }
}
