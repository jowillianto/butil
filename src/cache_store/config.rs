use crate::error::ConfigError;
use crate::prelude::AsyncToService;

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum Config {
    #[cfg(feature = "cache-store-memory")]
    InMemory,
    #[cfg(feature = "cache-store-redis")]
    Redis { endpoint: String, password: String },
}

#[async_trait::async_trait]
impl AsyncToService for Config {
    type Service = Box<dyn super::prelude::Service<Cache = String> + Send + Sync>;

    async fn to_service(&self) -> Result<Self::Service, ConfigError> {
        Ok(match self {
            #[cfg(feature = "cache-store-memory")]
            Self::InMemory => Box::new(super::in_memory::InMemory::<String>::new())
                as Box<dyn super::prelude::Service<Cache = String> + Send + Sync>,
            #[cfg(feature = "cache-store-redis")]
            Self::Redis { endpoint, .. } => Box::new(
                super::redis::RedisService::connect(endpoint.as_str())
                    .await
                    .map_err(|e| crate::config_error!("cache_store::Config", "redis: {}", e))?,
            ),
        })
    }
}
