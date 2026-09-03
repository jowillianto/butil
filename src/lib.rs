#[cfg(feature = "tokio")]
pub mod actor;
#[cfg(feature = "tokio")]
pub mod cache;
pub mod config_loader;
pub mod keyvec;
pub mod worker;
pub use config_loader::ConfigLoader;
pub use keyvec::{BoundedKeyVec, KeyVec};
pub use worker::Worker;

#[cfg(feature = "js")]
pub fn get_unix_timestamp_us() -> u64 {
    (js_sys::Date::now() * 1_000.0) as u64
}

#[cfg(not(feature = "js"))]
pub fn get_unix_timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_micros() as u64
}

pub use async_trait;

#[cfg(feature = "db-base")]
pub mod db;

#[cfg(feature = "fixture-loader")]
pub mod fixture_loader;

#[cfg(feature = "mail-base")]
pub mod mail;
#[cfg(feature = "mail-base")]
pub use mail::{Config as MailConfig, Ctx as MailCtx, Error as MailError};

#[cfg(feature = "storage-base")]
pub mod storage;

#[cfg(feature = "storage-base")]
pub use storage::{Config as StorageConfig, Service as StorageService};

#[cfg(feature = "db-base")]
pub use db::Config as DbConfig;

#[cfg(feature = "tokio")]
pub mod timer;
#[cfg(feature = "tokio")]
pub use timer::{wait_for, wait_or, wait_or_option};
