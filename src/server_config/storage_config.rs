use std::fmt::Display;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite};

#[derive(Debug)]
pub enum FsErrorKind {
    Unknown,
    Provider,
    Io,
    OutOfRange,
}

#[derive(Debug)]
pub struct FsError {
    pub kind: FsErrorKind,
    pub title: String,
    pub suggestions: Vec<String>,
}

impl FsError {
    pub fn new(kind: FsErrorKind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            suggestions: Vec::new(),
        }
    }
    pub fn suggest(mut self, suggestion: impl Into<String>) -> Self {
        self.suggest_mut(suggestion);
        self
    }
    pub fn suggest_mut(&mut self, suggestion: impl Into<String>) {
        self.suggestions.push(suggestion.into());
    }
}

impl Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FsError::{:?}: {}", self.kind, self.title)?;
        for suggestion in &self.suggestions {
            write!(f, "\n- suggest: {}", suggestion)?;
        }
        Ok(())
    }
}

impl std::error::Error for FsError {}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        FsError::new(FsErrorKind::Io, e.to_string())
    }
}

#[async_trait::async_trait]
pub trait Fs: Send + Sync {
    async fn rename(&self, from: &str, to: &str) -> Result<(), FsError>;
    async fn rm(&self, tgt: &str) -> Result<(), FsError>;
    async fn get_writable(&self, path: &str)
    -> Result<Box<dyn AsyncWrite + Send + Unpin>, FsError>;
    async fn get_readable(&self, path: &str) -> Result<Box<dyn AsyncRead + Send + Unpin>, FsError>;
    async fn get_bytes_at(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, FsError>;
    async fn get_size(&self, path: &str) -> Result<u64, FsError>;
    async fn get_local_readable(&self, path: &str) -> Result<Box<dyn LocalFile>, FsError>;
    async fn get_local_writable(
        &self,
        path: &str,
        truncate: bool,
    ) -> Result<Box<dyn LocalFile>, FsError>;
    fn get_subfs(&self, path: &str) -> Arc<dyn Fs>;
}

#[async_trait::async_trait]
pub trait LocalFile: Send + Sync {
    fn path(&self) -> &std::path::Path;
    async fn commit(self: Box<Self>) -> Result<(), FsError>;
}

pub struct LocalStorage {
    pub root: std::path::PathBuf,
}

impl LocalStorage {
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
    pub fn open_for_read(&self, p: &std::path::Path) -> Result<tokio::fs::File, tokio::io::Error> {
        std::fs::File::open(self.root.join(p)).map(tokio::fs::File::from_std)
    }
    pub fn open_for_write(&self, p: &std::path::Path) -> Result<tokio::fs::File, tokio::io::Error> {
        std::fs::File::create(self.root.join(p)).map(tokio::fs::File::from_std)
    }
}

#[async_trait::async_trait]
impl Fs for LocalStorage {
    async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        let to = self.root.join(to);
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        Ok(tokio::fs::rename(self.root.join(from), to).await?)
    }
    async fn rm(&self, tgt: &str) -> Result<(), FsError> {
        Ok(tokio::fs::remove_file(self.root.join(tgt)).await?)
    }
    async fn get_writable(
        &self,
        path: &str,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>, FsError> {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        Ok(Box::new(tokio::fs::File::create(path).await?))
    }
    async fn get_readable(&self, path: &str) -> Result<Box<dyn AsyncRead + Send + Unpin>, FsError> {
        Ok(Box::new(tokio::fs::File::open(self.root.join(path)).await?))
    }
    async fn get_bytes_at(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, FsError> {
        let mut file = tokio::fs::File::open(self.root.join(path)).await?;
        let size = file.metadata().await?.len();
        if offset >= size {
            return Err(FsError::new(
                FsErrorKind::OutOfRange,
                format!("offset {} is out of range for size {}", offset, size),
            ));
        }
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        let mut buf = Vec::new();
        file.take(len).read_to_end(&mut buf).await?;
        Ok(buf)
    }
    async fn get_size(&self, path: &str) -> Result<u64, FsError> {
        Ok(tokio::fs::metadata(self.root.join(path)).await?.len())
    }
    async fn get_local_readable(&self, path: &str) -> Result<Box<dyn LocalFile>, FsError> {
        let path = self.root.join(path);
        tokio::fs::metadata(&path).await?;
        Ok(Box::new(LocalStorageFile { path }))
    }
    async fn get_local_writable(
        &self,
        path: &str,
        truncate: bool,
    ) -> Result<Box<dyn LocalFile>, FsError> {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if truncate {
            tokio::fs::File::create(&path).await?;
        }
        Ok(Box::new(LocalStorageFile { path }))
    }
    fn get_subfs(&self, path: &str) -> Arc<dyn Fs> {
        Arc::new(LocalStorage {
            root: self.root.join(path),
        })
    }
}

pub struct LocalStorageFile {
    path: std::path::PathBuf,
}

#[async_trait::async_trait]
impl LocalFile for LocalStorageFile {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
    async fn commit(self: Box<Self>) -> Result<(), FsError> {
        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum Config {
    Local {
        root: std::path::PathBuf,
    },
    #[cfg(feature = "server-config-aws")]
    S3 {
        root: String,
    },
}

impl Config {
    pub async fn create_fs(&self) -> Result<Arc<dyn Fs>, FsError> {
        match self {
            Config::Local { root } => Ok(Arc::new(LocalStorage { root: root.clone() })),
            #[cfg(feature = "server-config-aws")]
            Config::S3 { root } => {
                let Some((bucket, prefix)) = root
                    .strip_prefix("s3://")
                    .map(|rest| rest.split_once('/').unwrap_or((rest, "")))
                else {
                    return Err(FsError::new(
                        FsErrorKind::Unknown,
                        format!("unsupported s3 root {}", root),
                    )
                    .suggest("use s3://<bucket>/<prefix>"));
                };
                Ok(Arc::new(
                    super::object_store_fs::ObjectStoreFs::try_new_s3_bucket(bucket, prefix)
                        .await?,
                ))
            }
        }
    }
}
