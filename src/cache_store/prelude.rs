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

pub trait IsExpired: Sized {
    fn is_expired(&self) -> bool;
    fn error_if_expire(self) -> Result<Self, Error> {
        if self.is_expired() {
            return Err(Error::expired("expired"));
        }
        Ok(self)
    }
    fn none_if_expire(self) -> Option<Self> {
        if self.is_expired() {
            return None;
        }
        Some(self)
    }
}

pub trait IntoValue {
    type Target;
    fn into_value(self) -> Self::Target;
}
