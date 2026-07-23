use std::fmt::Display;
use std::path::Path;
use tokio::io::AsyncRead;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Kind {
    NotFound,
    Io,
}

#[derive(Debug, Clone)]
pub struct Error {
    kind: Kind,
    reason: String,
}

impl Error {
    pub fn not_found(path: impl Display) -> Self {
        Self {
            kind: Kind::NotFound,
            reason: format!("{}", path),
        }
    }

    pub fn io(reason: impl Display) -> Self {
        Self {
            kind: Kind::Io,
            reason: format!("{}", reason),
        }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn reason(&self) -> &str {
        self.reason.as_str()
    }
}

impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> Self {
        Self::io(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "StorageError(kind: {:?}, err: {})",
            self.kind, self.reason
        )
    }
}

#[async_trait::async_trait]
pub trait Service {
    fn local_base_path(&self) -> &Path;
    async fn read(&self, p: &str) -> Result<Box<dyn AsyncRead + Unpin>, Error>;
    async fn write(&self, p: &str, f: Box<dyn AsyncRead + Send + Unpin>) -> Result<(), Error>;
}

pub struct Local {
    root: std::path::PathBuf,
}

impl Local {
    pub fn new(p: impl Into<std::path::PathBuf>) -> Self {
        Local { root: p.into() }
    }
}

#[async_trait::async_trait]
impl Service for Local {
    fn local_base_path(&self) -> &std::path::Path {
        &self.root
    }
    async fn read(&self, p: &str) -> Result<Box<dyn AsyncRead + Unpin>, Error> {
        let f = tokio::fs::File::open(self.root.join(p))
            .await
            .map_err(Error::io)?;
        Ok(Box::new(f))
    }
    async fn write(
        &self,
        p: &str,
        mut source: Box<dyn AsyncRead + Send + Unpin>,
    ) -> Result<(), Error> {
        let mut f = tokio::fs::File::create(self.root.join(p))
            .await
            .map_err(Error::io)?;
        tokio::io::copy(&mut source, &mut f)
            .await
            .map_err(Error::io)?;
        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct Config {
    root: std::path::PathBuf,
}

impl Config {
    pub fn build(&self) -> Local {
        Local::new(self.root.clone())
    }
}
