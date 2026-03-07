use super::error::Error;

#[async_trait::async_trait]
pub trait Service {
    type Cache;
    async fn get(&self, k: &str) -> Result<Option<Self::Cache>, Error>;
    async fn set(&self, k: &str, v: &Self::Cache) -> Result<(), Error>;
    async fn del(&self, k: &str) -> Result<bool, Error>;
}

pub trait ToCache {
    type Native;
    type Cache;
    fn to_cache(&self, v: &Self::Native) -> Result<Self::Cache, Error>;
}

pub trait FromCache {
    type Native;
    type Cache;
    fn from_cache(&self, v: &Self::Cache) -> Result<Self::Native, Error>;
}
