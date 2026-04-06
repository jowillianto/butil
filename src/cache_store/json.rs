use super::error::Error;
use super::prelude::{FromCache, ToCache};

#[derive(Debug, Copy, Clone)]
pub struct JsonTF<T: serde::Serialize + for<'de> serde::Deserialize<'de>> {
    marker: std::marker::PhantomData<T>,
}

impl<T: serde::Serialize + for<'de> serde::Deserialize<'de>> Default for JsonTF<T> {
    fn default() -> Self {
        Self {
            marker: std::marker::PhantomData,
        }
    }
}

impl<T: serde::Serialize + for<'de> serde::Deserialize<'de>> FromCache for JsonTF<T> {
    type Native = T;
    type Cache = String;
    fn from_cache(&self, v: &Self::Cache) -> Result<Self::Native, Error> {
        serde_json::from_str(v.as_str()).map_err(Error::from_cache)
    }
}

impl<T: serde::Serialize + for<'de> serde::Deserialize<'de>> ToCache for JsonTF<T> {
    type Native = T;
    type Cache = String;
    fn to_cache(&self, v: &Self::Native) -> Result<Self::Cache, Error> {
        serde_json::to_string(v).map_err(Error::to_cache)
    }
}
