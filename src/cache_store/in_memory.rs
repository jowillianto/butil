use std::collections::HashMap;

use super::error::Error;
use super::prelude::Service;

#[derive(Debug)]
pub struct InMemory<T> {
    inner: std::sync::RwLock<HashMap<String, T>>,
}

impl<T> InMemory<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T> Default for InMemory<T> {
    fn default() -> Self {
        Self {
            inner: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl<T: Clone> Service for InMemory<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Cache = T;

    async fn get(&self, k: &str) -> Result<Option<Self::Cache>, Error> {
        Ok(self.inner.read().expect("poisoned").get(k).cloned())
    }

    async fn set(&self, k: &str, v: &Self::Cache) -> Result<(), Error> {
        self.inner
            .write()
            .expect("poisoned")
            .insert(k.into(), v.clone());
        Ok(())
    }

    async fn del(&self, k: &str) -> Result<bool, Error> {
        let v = self.inner.write().expect("poisoned").remove(k);
        Ok(v.is_some())
    }
}

pub type InMemoryService = InMemory<String>;
