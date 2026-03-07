use clap::Parser;

use crate::s3::S3Path;

#[derive(Parser, Debug)]
#[command(name = "s3v")]
#[command(author, version, about = "A read-only S3 browser TUI", long_about = None)]
pub struct Cli {
    /// S3 path to open (bucket or bucket/prefix)
    #[arg(value_name = "PATH")]
    pub path: Option<String>,

    /// AWS profile to use
    #[arg(long, short)]
    pub profile: Option<String>,

    /// AWS region
    #[arg(long, short)]
    pub region: Option<String>,

    /// Custom S3 endpoint URL (for MinIO, LocalStack, etc.)
    #[arg(long)]
    pub endpoint: Option<String>,
}

impl Cli {
    pub fn initial_path(&self) -> S3Path {
        match &self.path {
            Some(path) => parse_s3_path(path),
            None => S3Path::root(),
        }
    }
}

fn parse_s3_path(path: &str) -> S3Path {
    // Remove s3:// prefix if present
    let path = path.strip_prefix("s3://").unwrap_or(path);

    if path.is_empty() {
        return S3Path::root();
    }

    // Split bucket and prefix
    match path.find('/') {
        Some(pos) => {
            let bucket = &path[..pos];
            let prefix = &path[pos + 1..];
            S3Path::with_prefix(bucket, prefix)
        }
        None => S3Path::bucket(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_s3_path_empty() {
        let path = parse_s3_path("");
        assert!(path.is_root());
    }

    #[test]
    fn test_parse_s3_path_bucket_only() {
        let path = parse_s3_path("my-bucket");
        assert_eq!(path.bucket, Some("my-bucket".to_string()));
        assert_eq!(path.prefix, "");
    }

    #[test]
    fn test_parse_s3_path_with_prefix() {
        let path = parse_s3_path("my-bucket/folder/subfolder/");
        assert_eq!(path.bucket, Some("my-bucket".to_string()));
        assert_eq!(path.prefix, "folder/subfolder/");
    }

    #[test]
    fn test_parse_s3_path_with_s3_uri() {
        let path = parse_s3_path("s3://my-bucket/folder/");
        assert_eq!(path.bucket, Some("my-bucket".to_string()));
        assert_eq!(path.prefix, "folder/");
    }
}
