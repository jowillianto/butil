extern crate self as butil;

pub mod error;
pub mod prelude;

pub use async_trait;
pub use butil_macro::AsyncToService;

#[cfg(feature = "db-base")]
pub mod db;

#[cfg(feature = "mail-base")]
pub mod mail;

#[cfg(feature = "cache-store-base")]
pub mod cache_store;

#[cfg(feature = "storage-base")]
pub mod storage;

// export
#[cfg(feature = "cache-store-base")]
pub use cache_store::config::Config as CacheConfig;

#[cfg(feature = "mail-base")]
pub use mail::config::Config as MailConfig;
#[cfg(feature = "storage-base")]
pub use storage::config::Config as StorageConfig;

#[cfg(feature = "db-base")]
pub use db::Config as DbConfig;
