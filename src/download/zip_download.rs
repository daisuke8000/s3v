use std::io::Write;
use std::path::{Path, PathBuf};

use aws_sdk_s3::Client;
use tokio::sync::mpsc;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::error::{Result, S3vError};

/// 10MB threshold for auto-directory creation
const AUTO_DIR_THRESHOLD: u64 = 10 * 1024 * 1024;

/// Determine zip file destination path.
/// If total_size > 10MB, creates s3v-{timestamp} subdirectory.
pub fn zip_destination(
    user_dest: &Path,
    archive_name: &str,
    total_size: u64,
    timestamp: &str,
) -> PathBuf {
    if total_size > AUTO_DIR_THRESHOLD {
        let dir_name = format!("s3v-{}", timestamp);
        user_dest
            .join(dir_name)
            .join(format!("{}.zip", archive_name))
    } else {
        user_dest.join(format!("{}.zip", archive_name))
    }
}

/// Write a single file entry into a zip archive.
/// Rejects paths containing `..` to prevent path traversal in zip entries.
pub fn write_zip_entry<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    relative_path: &str,
    data: &[u8],
) -> Result<()> {
    // Path traversal prevention
    if relative_path.split('/').any(|seg| seg == "..") {
        return Err(S3vError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Path traversal detected in zip entry path",
        )));
    }

    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file(relative_path, options)
        .map_err(|e| S3vError::Io(std::io::Error::other(e.to_string())))?;
    writer.write_all(data)?;
    Ok(())
}

/// Download multiple files from S3 and write them directly into a zip archive.
pub async fn download_as_zip(
    client: &Client,
    bucket: &str,
    keys: &[String],
    zip_path: &Path,
    base_prefix: &str,
    progress_tx: mpsc::UnboundedSender<(usize, usize, String)>,
) -> Result<()> {
    // Create parent directory if needed
    if let Some(parent) = zip_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let total = keys.len();

    for (i, key) in keys.iter().enumerate() {
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

        let relative = key.strip_prefix(base_prefix).unwrap_or(key);
        write_zip_entry(&mut zip, relative, &body.into_bytes())?;

        let file_name = key.split('/').next_back().unwrap_or(key).to_string();
        let _ = progress_tx.send((i + 1, total, file_name));
    }

    zip.finish()
        .map_err(|e| S3vError::Io(std::io::Error::other(e.to_string())))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_zip_destination_under_threshold() {
        let dest = Path::new("/tmp/downloads");
        let result = zip_destination(dest, "photos", 5 * 1024 * 1024, "20260308-120000");
        assert_eq!(result, PathBuf::from("/tmp/downloads/photos.zip"));
    }

    #[test]
    fn test_zip_destination_over_threshold() {
        let dest = Path::new("/tmp/downloads");
        let result = zip_destination(dest, "photos", 15 * 1024 * 1024, "20260308-120000");
        assert_eq!(
            result,
            PathBuf::from("/tmp/downloads/s3v-20260308-120000/photos.zip")
        );
    }

    #[test]
    fn test_zip_destination_exactly_threshold() {
        let dest = Path::new("/tmp/downloads");
        let result = zip_destination(dest, "data", 10 * 1024 * 1024, "20260308-120000");
        assert_eq!(result, PathBuf::from("/tmp/downloads/data.zip"));
    }

    #[test]
    fn test_write_zip_entry() {
        let buf = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(buf);
        let result = write_zip_entry(&mut writer, "test.txt", b"hello world");
        assert!(result.is_ok());
        let finished = writer.finish().unwrap();
        assert!(!finished.into_inner().is_empty());
    }

    #[test]
    fn test_write_zip_entry_rejects_path_traversal() {
        let buf = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(buf);
        let result = write_zip_entry(&mut writer, "../etc/passwd", b"bad");
        assert!(result.is_err());
    }

    #[test]
    fn test_write_multiple_entries() {
        let buf = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(buf);
        write_zip_entry(&mut writer, "dir/file1.txt", b"content1").unwrap();
        write_zip_entry(&mut writer, "dir/file2.txt", b"content2").unwrap();
        let finished = writer.finish().unwrap();
        let data = finished.into_inner();

        let reader = Cursor::new(data);
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        assert_eq!(archive.len(), 2);
        assert_eq!(archive.by_index(0).unwrap().name(), "dir/file1.txt");
        assert_eq!(archive.by_index(1).unwrap().name(), "dir/file2.txt");
    }
}
