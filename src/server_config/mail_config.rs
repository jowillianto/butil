use crate::actor::oneshot::{OneshotReceiver, oneshot};
use crate::actor::prelude::{ActorCtl, GetMailbox, Lifecycle};
use crate::actor::{ActorArg, ActorStatus, ActorStatusKind, ShutdownAction};
use std::collections::HashMap;
use std::fmt::Display;

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

pub trait ParseMailAck<Ctx> {
    fn parse_mail_ack(&self, ctx: &Ctx) -> Result<Option<String>, Error>;
}

pub struct JjinjaCss {
    template: String,
    ack: Option<String>,
}

impl JjinjaCss {
    pub async fn from_file(p: impl AsRef<std::path::Path>) -> Result<Self, tokio::io::Error> {
        let p = p.as_ref();
        let css_path = p.with_extension("css");
        let css = tokio::fs::read_to_string(&css_path).await?;
        let html = tokio::fs::read_to_string(p).await?;
        let ack_css = tokio::fs::read_to_string(format!("{}.ack", css_path.display()))
            .await
            .ok();
        let ack_html = tokio::fs::read_to_string(format!("{}.ack", p.display()))
            .await
            .ok();
        Ok(Self {
            template: html.replacen("</head>", &format!("<style>{}</style></head>", css), 1),
            ack: ack_html.zip(ack_css).map(|(html, css)| {
                html.replacen("</head>", &format!("<style>{}</style></head>", css), 1)
            }),
        })
    }
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            ack: None,
        }
    }
}

impl ParseMail<HashMap<String, String>> for JjinjaCss {
    fn parse_mail(&self, ctx: &HashMap<String, String>) -> Result<String, Error> {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.render_str(&self.template, ctx).map_err(|e| {
            Error::new("mail::template_render", e.to_string()).suggest(
                "check that every variable referenced by the template is present in the context",
            )
        })
    }
}

impl ParseMailAck<HashMap<String, String>> for JjinjaCss {
    fn parse_mail_ack(&self, ctx: &HashMap<String, String>) -> Result<Option<String>, Error> {
        let Some(ack) = self.ack.as_ref() else {
            return Ok(None);
        };
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.render_str(ack, ctx).map(Some).map_err(|e| {
            Error::new("mail::ack_render", e.to_string()).suggest(
                "check that every variable referenced by the ack template is present in the context",
            )
        })
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
    ) -> (Event, OneshotReceiver<Result<(), Error>>) {
        let (tx, rx) = oneshot(dur);
        let e = Event { tx, msg };
        (e, rx)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
enum MailTransportConfig {
    Smtp {
        url: String,
        username: String,
        password: String,
    },
    File {
        dir: std::path::PathBuf,
    },
}

fn default_usize<const N: usize>() -> usize {
    N
}

#[derive(serde::Deserialize)]
pub struct Config {
    #[serde(flatten)]
    transport: MailTransportConfig,
    #[serde(default = "default_usize::<2048>")]
    queue_size: usize,
    template_dir: std::path::PathBuf,
    pub template_ack: Vec<String>,
}

impl Config {
    pub fn factory(&self) -> JjinjaCssFactory {
        JjinjaCssFactory {
            dir: self.template_dir.clone(),
        }
    }
    pub fn create_service(&self) -> Result<MailService, lettre::transport::smtp::Error> {
        let config = ActorArg {
            shutdown_action: ShutdownAction::Drain,
            ..Default::default()
        };
        let (tx, rx) = tokio::sync::mpsc::channel(self.queue_size);
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let (worker, status) = match &self.transport {
            MailTransportConfig::Smtp {
                url,
                username,
                password,
            } => {
                let tp = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::from_url(url)?
                    .credentials(lettre::transport::smtp::authentication::Credentials::new(
                        username.clone(),
                        password.clone(),
                    ))
                    .build();
                config.run_with_lifecycle(MailContext { tranport: tp }, stream)
            }
            MailTransportConfig::File { dir } => {
                let tp = lettre::AsyncFileTransport::<lettre::Tokio1Executor>::new(dir);
                config.run_with_lifecycle(MailContext { tranport: tp }, stream)
            }
        };
        Ok(MailService {
            worker: tokio::sync::Mutex::new(worker),
            status,
            tx,
        })
    }
}

pub struct MailContext<T: lettre::AsyncTransport + Send + Sync> {
    tranport: T,
}

impl<T> Lifecycle<Event> for MailContext<T>
where
    T: lettre::AsyncTransport<Error: std::fmt::Display> + Send + Sync,
{
    async fn on_event(&mut self, e: Event) -> bool {
        let res = self
            .tranport
            .send(e.msg)
            .await
            .map(|_| ())
            .map_err(|e| Error::new("mail::send", e.to_string()));
        let _ = e.tx.send(res);
        true
    }

    async fn deinit(&mut self) {
        self.tranport.shutdown().await;
    }
}

pub struct MailService {
    worker: tokio::sync::Mutex<crate::async_utils::Worker<()>>,
    status: ActorStatus,
    tx: tokio::sync::mpsc::Sender<Event>,
}

impl MailService {
    pub fn mailbox(&self) -> tokio::sync::mpsc::Sender<Event> {
        self.tx.clone()
    }
}

impl GetMailbox for MailService {
    type M = tokio::sync::mpsc::Sender<Event>;
    fn get_mailbox(&self) -> Self::M {
        self.tx.clone()
    }
}

#[async_trait::async_trait]
impl ActorCtl for MailService {
    fn status(&self) -> ActorStatusKind {
        self.status.phase()
    }
    async fn stop(&self) {
        self.status.stop();
        self.worker.lock().await.cancel();
    }
    async fn wait(&self) {
        self.worker.lock().await.wait().await;
    }
}
