use std::io::Write;
use std::path::{Path, PathBuf};

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
pub fn write_zip_entry<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    relative_path: &str,
    data: &[u8],
) -> Result<()> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file(relative_path, options)
        .map_err(|e| S3vError::Io(std::io::Error::other(e.to_string())))?;
    writer
        .write_all(data)
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
