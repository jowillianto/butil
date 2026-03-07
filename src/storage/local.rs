use tokio::io::AsyncRead;

use super::error::Error;
use super::prelude::Service;

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
    fn base_path(&self) -> String {
        self.root.to_string_lossy().into()
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
