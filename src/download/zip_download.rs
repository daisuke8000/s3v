use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use aws_sdk_s3::Client;
use tokio::sync::mpsc;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::error::{Result, S3vError};

/// 10MB threshold for auto-directory creation
const AUTO_DIR_THRESHOLD: u64 = 10 * 1024 * 1024;

/// Generate timestamp string for download directories (YYYYMMDD-HHMMSS in UTC)
pub fn download_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let (year, month, day) = days_to_date(secs / 86400);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day) using Howard Hinnant's algorithm
fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    let z = days_since_epoch as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

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
/// Supports cancellation via the `cancel` flag. On error or cancellation,
/// the partial zip file is removed.
pub async fn download_as_zip(
    client: &Client,
    bucket: &str,
    keys: &[String],
    zip_path: &Path,
    base_prefix: &str,
    cancel: Arc<AtomicBool>,
    progress_tx: mpsc::UnboundedSender<(usize, usize, String)>,
) -> Result<()> {
    let result = download_as_zip_inner(
        client,
        bucket,
        keys,
        zip_path,
        base_prefix,
        &cancel,
        progress_tx,
    )
    .await;

    if result.is_err() || cancel.load(Ordering::Relaxed) {
        // Clean up partial zip file on error or cancellation
        let _ = std::fs::remove_file(zip_path);
    }

    result
}

async fn download_as_zip_inner(
    client: &Client,
    bucket: &str,
    keys: &[String],
    zip_path: &Path,
    base_prefix: &str,
    cancel: &AtomicBool,
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
        // Check for cancellation before each file
        if cancel.load(Ordering::Relaxed) {
            return Err(S3vError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Download cancelled",
            )));
        }

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

    // I5: days_to_date tests
    #[test]
    fn test_days_to_date_epoch() {
        // 1970-01-01
        assert_eq!(days_to_date(0), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_date_known_date_2024() {
        // 2024-03-08 = 19790 days since epoch
        assert_eq!(days_to_date(19790), (2024, 3, 8));
    }

    #[test]
    fn test_days_to_date_known_date_2026() {
        // 2026-03-08 = 20520 days since epoch
        assert_eq!(days_to_date(20520), (2026, 3, 8));
    }
}
