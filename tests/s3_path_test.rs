use s3v::s3::{S3Item, S3Path};

#[test]
fn test_s3_path_root() {
    let path = S3Path::root();
    assert!(path.is_root());
    assert_eq!(path.bucket, None);
    assert_eq!(path.prefix, "");
}

#[test]
fn test_s3_path_bucket() {
    let path = S3Path::bucket("my-bucket");
    assert!(!path.is_root());
    assert_eq!(path.bucket, Some("my-bucket".to_string()));
    assert_eq!(path.prefix, "");
}

#[test]
fn test_s3_path_with_prefix() {
    let path = S3Path::with_prefix("my-bucket", "folder/subfolder/");
    assert_eq!(path.bucket, Some("my-bucket".to_string()));
    assert_eq!(path.prefix, "folder/subfolder/");
}

#[test]
fn test_s3_path_parent_from_prefix() {
    let path = S3Path::with_prefix("my-bucket", "folder/subfolder/");
    let parent = path.parent().unwrap();
    assert_eq!(parent.bucket, Some("my-bucket".to_string()));
    assert_eq!(parent.prefix, "folder/");
}

#[test]
fn test_s3_path_parent_from_bucket() {
    let path = S3Path::bucket("my-bucket");
    let parent = path.parent().unwrap();
    assert!(parent.is_root());
}

#[test]
fn test_s3_path_parent_from_root() {
    let path = S3Path::root();
    assert!(path.parent().is_none());
}

#[test]
fn test_s3_path_join_from_root() {
    let path = S3Path::root();
    let joined = path.join("my-bucket");
    assert_eq!(joined.bucket, Some("my-bucket".to_string()));
    assert_eq!(joined.prefix, "");
}

#[test]
fn test_s3_path_join_folder() {
    let path = S3Path::bucket("my-bucket");
    let joined = path.join("folder/");
    assert_eq!(joined.prefix, "folder/");
}

#[test]
fn test_s3_path_to_s3_uri() {
    assert_eq!(S3Path::root().to_s3_uri(), "s3://");
    assert_eq!(S3Path::bucket("my-bucket").to_s3_uri(), "s3://my-bucket");
    assert_eq!(
        S3Path::with_prefix("my-bucket", "folder/file.txt").to_s3_uri(),
        "s3://my-bucket/folder/file.txt"
    );
}

#[test]
fn test_s3_path_display() {
    assert_eq!(format!("{}", S3Path::root()), "");
    assert_eq!(format!("{}", S3Path::bucket("my-bucket")), "my-bucket/");
    assert_eq!(
        format!("{}", S3Path::with_prefix("my-bucket", "folder/")),
        "my-bucket/folder/"
    );
}

#[test]
fn test_s3_item_bucket() {
    let item = S3Item::Bucket {
        name: "my-bucket".to_string(),
    };
    assert_eq!(item.name(), "my-bucket");
    assert!(item.is_folder());
    assert!(item.size().is_none());
}

#[test]
fn test_s3_item_file() {
    let item = S3Item::File {
        name: "file.txt".to_string(),
        key: "folder/file.txt".to_string(),
        size: 1024,
        last_modified: Some("2024-03-15".to_string()),
    };
    assert_eq!(item.name(), "file.txt");
    assert!(!item.is_folder());
    assert_eq!(item.size(), Some(1024));
    assert_eq!(item.last_modified(), Some("2024-03-15"));
}
