use crate::error::ConfigError;
use crate::prelude::AsyncToService;
use std::future::Future;

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
enum Provider {
    #[cfg(feature = "db-pg")]
    Postgres {
        host: String,
        username: String,
        password: String,
        name: String,
    },
    #[cfg(feature = "db-sqlite")]
    Sqlite {
        file: String,
        mode: String,
    },
    #[cfg(feature = "db-sqlite")]
    InMemory {},
}

fn default_u32<const N: u32>() -> u32 {
    N
}
fn default_u64<const N: u64>() -> u64 {
    N
}

#[derive(Debug, serde::Deserialize)]
pub struct Config {
    #[serde(flatten)]
    provider: Provider,
    #[serde(default = "default_u32::<50>")]
    max_connections: u32,
    #[serde(default = "default_u32::<10>")]
    min_connections: u32,
    #[serde(default = "default_u64::<10>")]
    connect_timeout: u64,
    #[serde(default = "default_u64::<20>")]
    acquire_timeout: u64,
    #[serde(default = "default_u64::<600>")]
    idle_timeout: u64,
    #[serde(default = "default_u64::<600>")]
    max_lifetime: u64,
}

impl Config {
    pub fn connect_options(&self) -> sea_orm::ConnectOptions {
        match &self.provider {
            Provider::Postgres {
                host,
                username,
                password,
                name,
            } => sea_orm::ConnectOptions::new(format!(
                "postgres://{}:{}@{}/{}",
                username, password, host, name
            )),
            Provider::Sqlite { file, mode } => {
                sea_orm::ConnectOptions::new(format!("sqlite://{}?mode={}", file, mode))
            }
            Provider::InMemory {} => sea_orm::ConnectOptions::new("sqlite::memory:"),
        }
        .max_connections(self.max_connections)
        .min_connections(self.min_connections)
        .connect_timeout(std::time::Duration::from_secs(self.connect_timeout))
        .idle_timeout(std::time::Duration::from_secs(self.idle_timeout))
        .acquire_timeout(std::time::Duration::from_secs(self.acquire_timeout))
        .connect_timeout(std::time::Duration::from_secs(self.connect_timeout))
        .max_lifetime(std::time::Duration::from_secs(self.max_lifetime))
        .to_owned()
    }

    pub async fn connect(&self) -> Result<sea_orm::DatabaseConnection, sea_orm::DbErr> {
        sea_orm::Database::connect(self.connect_options()).await
    }
}

impl AsyncToService for Config {
    type Service = sea_orm::DatabaseConnection;

    fn to_service(
        &self,
    ) -> impl Future<Output = Result<Self::Service, ConfigError>> {
        async move {
            self.connect()
                .await
                .map_err(|e| crate::config_error!("db::Config", "connect: {}", e))
        }
    }
}
