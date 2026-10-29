use std::sync::Arc;

use object_store::ObjectStoreExt;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use super::storage_config::{Fs, FsError, FsErrorKind, LocalFile, SubFs};

impl From<object_store::Error> for FsError {
    fn from(e: object_store::Error) -> Self {
        FsError::new(FsErrorKind::Provider, e.to_string())
    }
}

type Handle = Arc<dyn object_store::ObjectStore>;

#[derive(Clone)]
pub struct ObjectStoreFs {
    handle: Handle,
    remote_prefix: object_store::path::Path,
}

impl ObjectStoreFs {
    pub async fn try_new_handle(
        handle: Arc<dyn object_store::ObjectStore>,
        prefix: impl Into<String>,
    ) -> Result<Self, FsError> {
        Ok(Self {
            handle,
            remote_prefix: object_store::path::Path::from(prefix.into()),
        })
    }
    pub async fn try_new_s3_bucket(
        bucket: &str,
        prefix: impl Into<String>,
    ) -> Result<Self, FsError> {
        let s3 = object_store::aws::AmazonS3Builder::from_env()
            .with_bucket_name(bucket)
            .build()?;
        Self::try_new_handle(Arc::new(s3), prefix).await
    }
    fn remote_path(&self, path: &str) -> object_store::path::Path {
        self.remote_prefix
            .parts()
            .chain(object_store::path::Path::from(path).parts())
            .collect()
    }
}

#[async_trait::async_trait]
impl Fs for ObjectStoreFs {
    async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        Ok(self
            .handle
            .rename(&self.remote_path(from), &self.remote_path(to))
            .await?)
    }
    async fn rm(&self, tgt: &str) -> Result<(), FsError> {
        Ok(self.handle.delete(&self.remote_path(tgt)).await?)
    }
    async fn get_writable(
        &self,
        path: &str,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>, FsError> {
        Ok(Box::new(object_store::buffered::BufWriter::new(
            self.handle.clone(),
            self.remote_path(path),
        )))
    }
    async fn get_readable(&self, path: &str) -> Result<Box<dyn AsyncRead + Send + Unpin>, FsError> {
        let res = self.handle.get(&self.remote_path(path)).await?;
        Ok(Box::new(tokio_util::io::StreamReader::new(
            res.into_stream(),
        )))
    }
    async fn get_bytes_at(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, FsError> {
        let bytes = self
            .handle
            .get_range(&self.remote_path(path), offset..offset.saturating_add(len))
            .await?;
        Ok(bytes.into())
    }
    async fn get_size(&self, path: &str) -> Result<u64, FsError> {
        Ok(self.handle.head(&self.remote_path(path)).await?.size)
    }
    async fn get_local_readable(&self, path: &str) -> Result<Box<dyn LocalFile>, FsError> {
        let res = self.handle.get(&self.remote_path(path)).await?;
        let file = LocalObjectFileRO {
            runtime: tokio::runtime::Handle::current(),
            local: Some(
                std::env::temp_dir()
                    .join(uuid::Uuid::new_v4().to_string())
                    .with_extension(std::path::Path::new(path).extension().unwrap_or_default()),
            ),
        };
        let mut out = tokio::fs::File::create(file.path()).await?;
        tokio::io::copy(
            &mut tokio_util::io::StreamReader::new(res.into_stream()),
            &mut out,
        )
        .await?;
        out.flush().await?;
        Ok(Box::new(file))
    }
    async fn get_local_writable(
        &self,
        path: &str,
        truncate: bool,
    ) -> Result<Box<dyn LocalFile>, FsError> {
        let remote = self.remote_path(path);
        let mut staged = LocalObjectFileRO {
            runtime: tokio::runtime::Handle::current(),
            local: Some(
                std::env::temp_dir()
                    .join(uuid::Uuid::new_v4().to_string())
                    .with_extension(std::path::Path::new(path).extension().unwrap_or_default()),
            ),
        };
        if truncate {
            tokio::fs::File::create(staged.path()).await?;
        } else {
            match self.handle.get(&remote).await {
                Ok(res) => {
                    let mut out = tokio::fs::File::create(staged.path()).await?;
                    tokio::io::copy(
                        &mut tokio_util::io::StreamReader::new(res.into_stream()),
                        &mut out,
                    )
                    .await?;
                    out.flush().await?;
                }
                Err(object_store::Error::NotFound { .. }) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Box::new(LocalObjectFileRw {
            runtime: tokio::runtime::Handle::current(),
            fs: self.clone(),
            local: staged.local.take(),
            remote,
        }))
    }
}

impl SubFs for ObjectStoreFs {
    fn get_subfs(&self, path: &str) -> Arc<dyn Fs> {
        Arc::new(ObjectStoreFs {
            handle: self.handle.clone(),
            remote_prefix: self.remote_path(path),
        })
    }
}

pub struct LocalObjectFileRO {
    runtime: tokio::runtime::Handle,
    local: Option<std::path::PathBuf>,
}

#[async_trait::async_trait]
impl LocalFile for LocalObjectFileRO {
    fn path(&self) -> &std::path::Path {
        self.local.as_deref().expect("path() called after commit()")
    }
    async fn commit(mut self: Box<Self>) -> Result<(), FsError> {
        if let Some(local) = self.local.take() {
            tokio::fs::remove_file(local).await?;
        }
        Ok(())
    }
}

impl Drop for LocalObjectFileRO {
    fn drop(&mut self) {
        if let Some(local) = self.local.take() {
            self.runtime.spawn(async move {
                let _ = tokio::fs::remove_file(local).await;
            });
        }
    }
}

pub struct LocalObjectFileRw {
    runtime: tokio::runtime::Handle,
    fs: ObjectStoreFs,
    local: Option<std::path::PathBuf>,
    remote: object_store::path::Path,
}

#[async_trait::async_trait]
impl LocalFile for LocalObjectFileRw {
    fn path(&self) -> &std::path::Path {
        self.local.as_deref().expect("path() called after commit()")
    }
    async fn commit(mut self: Box<Self>) -> Result<(), FsError> {
        let Some(local) = self.local.as_deref() else {
            return Ok(());
        };
        let mut file = tokio::fs::File::open(local).await?;
        let mut writer =
            object_store::buffered::BufWriter::new(self.fs.handle.clone(), self.remote.clone());
        tokio::io::copy(&mut file, &mut writer).await?;
        writer.shutdown().await?;
        tokio::fs::remove_file(local).await?;
        self.local = None;
        Ok(())
    }
}

impl Drop for LocalObjectFileRw {
    fn drop(&mut self) {
        let Some(local) = self.local.take() else {
            return;
        };
        let handle = self.fs.handle.clone();
        let remote = std::mem::take(&mut self.remote);
        self.runtime.spawn(async move {
            let Ok(mut file) = tokio::fs::File::open(&local).await else {
                return;
            };
            let mut writer = object_store::buffered::BufWriter::new(handle, remote);
            if tokio::io::copy(&mut file, &mut writer).await.is_err()
                || writer.shutdown().await.is_err()
            {
                return;
            }
            let _ = tokio::fs::remove_file(local).await;
        });
    }
}
