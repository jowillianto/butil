use super::actor::{Actor, ActorConfig, Context, Receiver, ShutdownAction, new_pair};
use std::collections::HashMap;
use std::fmt::Display;

/*
 * Error is the normal error kind for mails
 */
#[derive(Debug)]
pub struct Error {
    kind: String,
    message: String,
    suggestions: Vec<String>,
}

impl Error {
    pub fn new(kind: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: msg.into(),
            suggestions: Vec::new(),
        }
    }
    pub fn suggest(mut self, suggestion: impl Into<String>) -> Self {
        self.suggest_mut(suggestion);
        self
    }
    pub fn suggest_mut(&mut self, suggestion: impl Into<String>) {
        self.suggestions.push(suggestion.into());
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MailError::{}: {}", self.kind, self.message)?;
        for suggestion in &self.suggestions {
            write!(f, "\n- suggest: {}", suggestion)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

pub struct MailEnv {
    inner: HashMap<String, String>,
}

impl MailEnv {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
    pub fn add_env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.add_env_mut(k, v);
        self
    }
    pub fn add_env_mut(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.inner.insert(k.into(), v.into());
    }
}

impl Default for MailEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl From<HashMap<String, String>> for MailEnv {
    fn from(inner: HashMap<String, String>) -> Self {
        Self { inner }
    }
}

impl AsRef<HashMap<String, String>> for MailEnv {
    fn as_ref(&self) -> &HashMap<String, String> {
        &self.inner
    }
}

pub trait ParseMail<Ctx> {
    fn parse_mail(&self, ctx: &Ctx) -> Result<String, Error>;
}

/*
 * Jjinja template substitution
 */
pub struct JjinjaCss {
    template: String,
}

impl JjinjaCss {
    pub async fn from_file(p: impl AsRef<std::path::Path>) -> Result<Self, tokio::io::Error> {
        let p = p.as_ref();
        let html = tokio::fs::read_to_string(p).await?;
        let css = tokio::fs::read_to_string(p.with_extension("css")).await?;
        Ok(Self {
            template: html.replacen("</head>", &format!("<style>{}</style></head>", css), 1),
        })
    }
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
        }
    }
}

impl ParseMail<HashMap<String, String>> for JjinjaCss {
    fn parse_mail(&self, ctx: &HashMap<String, String>) -> Result<String, Error> {
        /*
         * substitute environment
         */
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.render_str(&self.template, ctx).map_err(|e| {
            Error::new("mail::template_render", e.to_string()).suggest(
                "check that every variable referenced by the template is present in the context",
            )
        })
        /*
         * run tailwind css
         */
    }
}

pub struct JjinjaCssFactory {
    dir: std::path::PathBuf,
}

impl JjinjaCssFactory {
    pub async fn open_at(
        &self,
        p: impl AsRef<std::path::Path>,
    ) -> Result<JjinjaCss, tokio::io::Error> {
        JjinjaCss::from_file(self.dir.join(p).with_extension("html")).await
    }
}

/*
 * Mail events constructed via lettre
 */
pub struct Event {
    msg: lettre::Message,
    tx: tokio::sync::oneshot::Sender<Result<(), Error>>,
}

impl Event {
    pub fn new_ignore(msg: lettre::Message) -> Event {
        let (tx, _) = tokio::sync::oneshot::channel();
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
            .map_err(|e| Error::new("mail::send", e.to_string()));
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
        templates: String,
    },
    #[cfg(feature = "mail-file")]
    File {
        dir: std::path::PathBuf,
        #[serde(default = "default_usize::<2048>")]
        queue_size: usize,
        templates: String,
    },
}

impl Config {
    pub fn factory(&self) -> JjinjaCssFactory {
        JjinjaCssFactory {
            dir: match self {
                #[cfg(feature = "mail-smtp")]
                Config::Smtp { templates, .. } => templates.into(),
                #[cfg(feature = "mail-file")]
                Config::File { templates, .. } => templates.into(),
            },
        }
    }
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
                ..
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
            Config::File {
                dir, queue_size, ..
            } => {
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
