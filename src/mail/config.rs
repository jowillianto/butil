use crate::error::ConfigError;

use super::super::actor::{Actor, ActorConfig, ShutdownAction};
use super::queue::{Ctx, Event};

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

impl Config {
    pub fn build(
        &self,
    ) -> Result<(Actor<Event>, tokio::sync::mpsc::Sender<Event>), ConfigError> {
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
                Ok(Actor::new_bounded(
                    ActorConfig {
                        shutdown_action: ShutdownAction::Drain,
                    },
                    *queue_size,
                    Ctx { transport: tp },
                ))
            }
            #[cfg(feature = "mail-file")]
            Config::File { dir, queue_size } => {
                let tp = lettre::AsyncFileTransport::<lettre::Tokio1Executor>::new(dir);
                Ok(Actor::new_bounded(
                    ActorConfig {
                        shutdown_action: ShutdownAction::Drain,
                    },
                    *queue_size,
                    Ctx { transport: tp },
                ))
            }
        }
    }
}
