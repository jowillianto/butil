use super::local::Local;
use super::prelude::Service;
use crate::error::ConfigError;
use crate::prelude::AsyncToService;

#[derive(Debug, serde::Deserialize)]
struct S3Config {
    bucket: String,
    region: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct LocalConfig {
    root: std::path::PathBuf,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum Config {
    Local(LocalConfig),
    // S3(S3Config),
}

#[async_trait::async_trait]
impl AsyncToService for Config {
    type Service = Box<dyn Service + Send + Sync>;
    async fn to_service(&self) -> Result<Self::Service, ConfigError> {
        match self {
            Config::Local(conf) => Ok(Box::new(Local::new(conf.root.clone())) as Self::Service),
        }
    }
}
