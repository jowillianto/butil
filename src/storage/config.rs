use super::local::Local;
use super::prelude::Service;
use crate::error::ConfigError;
use crate::prelude::AsyncToService;
use std::future::Future;

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

impl AsyncToService for Config {
    type Service = Box<dyn Service + Send + Sync>;
    fn to_service(
        &self,
    ) -> impl Future<Output = Result<Self::Service, ConfigError>> {
        async move {
            match self {
                Config::Local(conf) => {
                    Ok(Box::new(Local::new(conf.root.clone())) as Self::Service)
                }
            }
        }
    }
}
