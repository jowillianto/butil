#[cfg(feature = "server-config-db")]
pub mod db_config;
#[cfg(feature = "server-config-fixture")]
pub mod fixture_loader;
#[cfg(feature = "server-config-loader")]
pub mod loader;
#[cfg(feature = "server-config-mail")]
pub mod mail_config;
#[cfg(feature = "server-config-aws")]
pub mod object_store_fs;
#[cfg(feature = "server-config-storage")]
pub mod storage_config;

#[cfg(feature = "server-config-db")]
pub use db_config::Config as DbConfig;
#[cfg(feature = "server-config-storage")]
pub use storage_config::Config as StorageConfig;
#[cfg(feature = "server-config-storage")]
pub use storage_config::LocalStorage;
