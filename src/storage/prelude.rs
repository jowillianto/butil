use std::path::Path;

use super::error::Error;
use tokio::io::AsyncRead;

#[async_trait::async_trait]
pub trait Service {
    fn local_base_path(&self) -> &Path;
    async fn read(&self, p: &str) -> Result<Box<dyn AsyncRead + Unpin>, Error>;
    async fn write(&self, p: &str, f: Box<dyn AsyncRead + Send + Unpin>) -> Result<(), Error>;
}
