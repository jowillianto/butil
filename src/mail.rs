use super::actor::{Actor, ActorConfig, Context, Receiver, ShutdownAction, new_pair};
use std::collections::HashMap;
use std::fmt::Display;

#[derive(Debug)]
pub struct Error {
    reason: String,
}

impl Error {
    pub fn new(e: impl Display) -> Self {
        Self {
            reason: format!("{}", e),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MailError({})", self.reason)
    }
}

#[derive(Debug)]
pub struct TemplateError {
    reason: String,
}

impl TemplateError {
    pub fn parse(e: impl Display) -> Self {
        Self {
            reason: format!("{}", e),
        }
    }
}

impl Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MailTemplateError(err: {})", self.reason)
    }
}

pub trait ParseMailTemplate<Ctx> {
    fn parse_mail_template(&self, ctx: &Ctx) -> Result<String, TemplateError>;
}

pub struct JjinjaTemplate {
    template: String,
}

impl ParseMailTemplate<HashMap<String, String>> for JjinjaTemplate {
    fn parse_mail_template(&self, ctx: &HashMap<String, String>) -> Result<String, TemplateError> {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.render_str(&self.template, ctx).map_err(TemplateError::parse)
    }
}

pub struct Event {
    msg: lettre::Message,
    tx: tokio::sync::oneshot::Sender<Result<(), Error>>,
}

impl Event {
    pub fn new_ignore(msg: lettre::Message) -> Event {
        let (tx, _) = new_pair(tokio::time::Duration::from_secs(0));
        Event { tx, msg }
    }
    pub fn new_with_rx(
        msg: lettre::Message,
        dur: tokio::time::Duration,
    ) -> (Event, Receiver<Result<(), Error>>) {
        let (tx, rx) = new_pair(dur);
        let e = Event { tx, msg };
        (e, rx)
    }
}

pub struct Ctx<T> {
    pub transport: T,
}

impl<T> Context<Event> for Ctx<T>
where
    T: lettre::AsyncTransport<Error: std::fmt::Display> + Send + Sync,
{
    async fn on_event(&mut self, e: Event) -> bool {
        let res = self
            .transport
            .send(e.msg)
            .await
            .map(|_| ())
            .map_err(Error::new);
        let _ = e.tx.send(res);
        true
    }

    async fn deinit(&mut self) {
        self.transport.shutdown().await;
    }
}

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
    ) -> Result<(Actor<Event>, tokio::sync::mpsc::Sender<Event>), lettre::transport::smtp::Error>
    {
        match self {
            #[cfg(feature = "mail-smtp")]
            Config::Smtp {
                url,
                username,
                password,
                queue_size,
            } => {
                let tp = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::from_url(url)?
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
