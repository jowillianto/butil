use super::error::Error;
use super::in_memory::InMemory;
use super::prelude::Service;
use super::redis::Redis;
use crate::error::ConfigError;
use crate::prelude::AsyncToService;
use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum Config {
    #[cfg(feature = "cache-store-memory")]
    InMemory,
    #[cfg(feature = "cache-store-redis")]
    Redis { endpoint: String, password: String },
}

#[derive(Clone)]
pub struct PolyService {
    inner: Arc<dyn super::prelude::Service<Cache = String> + Send + Sync>,
}

#[async_trait::async_trait]
impl Service for PolyService {
    type Cache = String;
    async fn get(&self, k: &str) -> Result<Option<Self::Cache>, Error> {
        self.inner.get(k).await
    }
    async fn set(&self, k: &str, v: &Self::Cache) -> Result<(), Error> {
        self.inner.set(k, v).await
    }
    async fn del(&self, k: &str) -> Result<bool, Error> {
        self.inner.del(k).await
    }
}

impl Deref for PolyService {
    type Target = dyn Service<Cache = String> + Send + Sync;
    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl AsyncToService for Config {
    type Service = PolyService;

    fn to_service(&self) -> impl Future<Output = Result<Self::Service, ConfigError>> {
        async move {
            let service = match self {
                #[cfg(feature = "cache-store-memory")]
                Self::InMemory => PolyService {
                    inner: Arc::new(InMemory::<String>::new()),
                },
                #[cfg(feature = "cache-store-redis")]
                Self::Redis { endpoint, .. } => PolyService {
                    inner: Arc::new(Redis::connect(endpoint.as_str()).await.map_err(|e| {
                        crate::config_error!("cache_store::Config", "redis: {}", e)
                    })?),
                },
            };
            Ok(service)
        }
    }
}
