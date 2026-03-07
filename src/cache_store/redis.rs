use redis::AsyncTypedCommands;

use super::{error::Error, prelude::Service};

pub struct RedisService {
    conn: redis::aio::MultiplexedConnection,
}

impl RedisService {
    pub async fn connect(
        conn_info: impl redis::IntoConnectionInfo,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(conn_info)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }
}

#[async_trait::async_trait]
impl Service for RedisService {
    type Cache = String;

    async fn get(&self, k: &str) -> Result<Option<Self::Cache>, Error> {
        self.conn.clone().get(k).await.map_err(Error::transport)
    }

    async fn set(&self, k: &str, v: &Self::Cache) -> Result<(), Error> {
        self.conn.clone().set(k, v).await.map_err(Error::transport)
    }

    async fn del(&self, k: &str) -> Result<bool, Error> {
        let deleted = self.conn.clone().del(k).await.map_err(Error::transport)?;
        Ok(deleted != 0)
    }
}
