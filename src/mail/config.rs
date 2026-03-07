use crate::error::ConfigError;
use crate::prelude::AsyncToService;

use super::queue::Queue;

fn default_usize<const N: usize>() -> usize {
    N
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum Config {
    #[cfg(feature = "mail-smtp")]
    Smtp {
        url: String,
        username: String,
        password: String,
        #[serde(default = "default_usize::<2048>")]
        queue_size: usize,
    },
    #[cfg(feature = "mail-file")]
    File {
        dir: std::path::PathBuf,
        #[serde(default = "default_usize::<2048>")]
        queue_size: usize,
    },
}

#[async_trait::async_trait]
impl AsyncToService for Config {
    type Service = Queue;

    async fn to_service(&self) -> Result<Self::Service, ConfigError> {
        match self {
            #[cfg(feature = "mail-smtp")]
            Config::Smtp {
                url,
                username,
                password,
                queue_size,
            } => {
                let tp = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::from_url(url)
                    .map_err(|e| crate::config_error!("mail::Config", "smtp-url: {}", e))?
                    .credentials(lettre::transport::smtp::authentication::Credentials::new(
                        username.clone(),
                        password.clone(),
                    ))
                    .build();
                Ok(Queue::new(tp, *queue_size, |_| async {}))
            }
            #[cfg(feature = "mail-file")]
            Config::File { dir, queue_size } => {
                let tp = lettre::AsyncFileTransport::<lettre::Tokio1Executor>::new(dir);
                Ok(Queue::new(tp, *queue_size, |_| async {}))
            }
        }
    }
}
