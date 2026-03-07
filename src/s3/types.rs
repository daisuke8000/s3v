use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct S3Path {
    pub bucket: Option<String>,
    pub prefix: String,
}

impl S3Path {
    pub fn root() -> Self {
        Self {
            bucket: None,
            prefix: String::new(),
        }
    }

    pub fn bucket(name: impl Into<String>) -> Self {
        Self {
            bucket: Some(name.into()),
            prefix: String::new(),
        }
    }

    pub fn with_prefix(bucket: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self {
            bucket: Some(bucket.into()),
            prefix: prefix.into(),
        }
    }

    pub fn is_root(&self) -> bool {
        self.bucket.is_none()
    }

    pub fn parent(&self) -> Option<S3Path> {
        let bucket = self.bucket.clone()?;

        if self.prefix.is_empty() {
            return Some(S3Path::root());
        }

        let trimmed = self.prefix.trim_end_matches('/');
        match trimmed.rfind('/') {
            Some(pos) => Some(S3Path::with_prefix(bucket, &trimmed[..=pos])),
            None => Some(S3Path::bucket(bucket)),
        }
    }

    pub fn join(&self, name: &str) -> S3Path {
        if let Some(bucket) = &self.bucket {
            S3Path::with_prefix(bucket, format!("{}{}", self.prefix, name))
        } else {
            S3Path::bucket(name)
        }
    }

    pub fn to_s3_uri(&self) -> String {
        match &self.bucket {
            Some(bucket) if self.prefix.is_empty() => format!("s3://{}", bucket),
            Some(bucket) => format!("s3://{}/{}", bucket, self.prefix),
            None => "s3://".to_string(),
        }
    }
}

impl fmt::Display for S3Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.bucket {
            Some(bucket) if self.prefix.is_empty() => write!(f, "{}/", bucket),
            Some(bucket) => write!(f, "{}/{}", bucket, self.prefix),
            None => write!(f, ""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S3Item {
    Bucket {
        name: String,
    },
    Folder {
        name: String,
        prefix: String,
    },
    File {
        name: String,
        key: String,
        size: u64,
        last_modified: Option<String>,
    },
}

impl S3Item {
    pub fn name(&self) -> &str {
        match self {
            S3Item::Bucket { name } => name,
            S3Item::Folder { name, .. } => name,
            S3Item::File { name, .. } => name,
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, S3Item::Bucket { .. } | S3Item::Folder { .. })
    }

    pub fn size(&self) -> Option<u64> {
        match self {
            S3Item::File { size, .. } => Some(*size),
            _ => None,
        }
    }

    pub fn last_modified(&self) -> Option<&str> {
        match self {
            S3Item::File { last_modified, .. } => last_modified.as_deref(),
            _ => None,
        }
    }
}
