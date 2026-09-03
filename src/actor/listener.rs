use std::sync::Arc;

use super::actor::{Actor, ActorConfig, ActorStatusKind, Context};
use super::timed_receiver::TimedReceiver;
use crate::KeyVec;

pub trait ActorSender<M: Send + 'static>: Send + Sync {
    fn send(&self, e: M);
}

impl<M: Send + 'static> ActorSender<M> for tokio::sync::mpsc::Sender<M> {
    fn send(&self, e: M) {
        let _ = tokio::sync::mpsc::Sender::try_send(self, e);
    }
}

pub enum ListenerEvent<E: Send + Sync + 'static> {
    Reg {
        id: usize,
        tx: Arc<dyn ActorSender<Arc<E>>>,
    },
    Unreg {
        id: usize,
    },
    Notify {
        event: Arc<E>,
    },
    Len {
        tx: tokio::sync::oneshot::Sender<usize>,
    },
}

impl<E: Send + Sync + 'static> ListenerEvent<E> {
    pub fn reg(tx: impl 'static + ActorSender<Arc<E>>) -> (Self, usize) {
        let tx = Arc::new(tx) as Arc<dyn ActorSender<Arc<E>>>;
        let id = (tx.as_ref() as *const dyn ActorSender<Arc<E>>).addr();
        (Self::Reg { id, tx }, id)
    }
    pub fn unreg(id: usize) -> Self {
        Self::Unreg { id }
    }
    pub fn len(timeout: tokio::time::Duration) -> (Self, TimedReceiver<usize>) {
        let (tx, rx) = TimedReceiver::new(timeout);
        (Self::Len { tx: tx }, rx)
    }
    pub fn notify(event: E) -> Self {
        Self::Notify {
            event: Arc::new(event),
        }
    }
}

impl<E: Send + Sync + 'static> std::fmt::Debug for ListenerEvent<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reg { id, .. } => f.debug_struct("Reg").field("id", id).finish(),
            Self::Unreg { id } => f.debug_struct("Unreg").field("id", id).finish(),
            Self::Notify { .. } => f.debug_struct("Notify").finish(),
            Self::Len { .. } => f.debug_struct("Len").finish(),
        }
    }
}

pub struct ListenerMailbox<E: Send + Sync + 'static> {
    tx: tokio::sync::mpsc::Sender<ListenerEvent<E>>,
    timeout: tokio::time::Duration,
}

impl<E: Send + Sync + 'static> Clone for ListenerMailbox<E> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            timeout: self.timeout,
        }
    }
}

impl<E: Send + Sync + 'static> ListenerMailbox<E> {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<ListenerEvent<E>>,
        timeout: tokio::time::Duration,
    ) -> Self {
        Self { tx, timeout }
    }
    pub fn tx(&self) -> tokio::sync::mpsc::Sender<ListenerEvent<E>> {
        self.tx.clone()
    }
    pub async fn notify(&self, event: E) -> bool {
        self.tx.send(ListenerEvent::notify(event)).await.is_ok()
    }
    pub async fn reg(&self, tx: impl 'static + ActorSender<Arc<E>>) -> Option<usize> {
        let (e, id) = ListenerEvent::reg(tx);
        self.tx.send(e).await.ok().map(|_| id)
    }
    pub async fn unreg(&self, id: usize) -> bool {
        self.tx.send(ListenerEvent::unreg(id)).await.is_ok()
    }
    pub async fn len(&self) -> Option<usize> {
        let (e, rx) = ListenerEvent::len(self.timeout);
        self.tx.send(e).await.ok()?;
        rx.recv().await.ok()
    }
}

pub struct ListenerCtx<E: Send + Sync + 'static> {
    listeners: KeyVec<usize, Arc<dyn ActorSender<Arc<E>>>>,
}

impl<E: Send + Sync + 'static> ListenerCtx<E> {
    pub fn new() -> Self {
        Self {
            listeners: KeyVec::default(),
        }
    }
}

impl<E: Send + Sync + 'static> Default for ListenerCtx<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Send + Sync + 'static> Context<ListenerEvent<E>> for ListenerCtx<E> {
    async fn init(&mut self) {}

    async fn on_event(&mut self, e: ListenerEvent<E>) -> bool {
        match e {
            ListenerEvent::Reg { id, tx } => {
                self.listeners.insert_no_check(id, tx);
                true
            }
            ListenerEvent::Unreg { id } => {
                self.listeners.remove(&id);
                true
            }
            ListenerEvent::Notify { event } => {
                for (_, tx) in self.listeners.iter() {
                    tx.send(event.clone());
                }
                true
            }
            ListenerEvent::Len { tx } => {
                let _ = tx.send(self.listeners.len());
                true
            }
        }
    }

    async fn deinit(&mut self) {}

    fn is_complete(&self) -> bool {
        false
    }
}

pub struct ListenerActor<E: Send + Sync + 'static> {
    actor: Actor<ListenerEvent<E>>,
    mailbox: ListenerMailbox<E>,
}

impl<E: Send + Sync + 'static> ListenerActor<E> {
    pub fn new(config: ActorConfig, buf_size: usize, timeout: tokio::time::Duration) -> Self {
        let (actor, tx) = Actor::new_bounded(config, buf_size, ListenerCtx::new());
        Self {
            actor,
            mailbox: ListenerMailbox::new(tx, timeout),
        }
    }
    pub fn mailbox(&self) -> ListenerMailbox<E> {
        self.mailbox.clone()
    }
    pub async fn stop(&self) {
        self.actor.stop().await;
    }
    pub async fn wait(&self) {
        self.actor.wait().await;
    }
    pub fn status(&self) -> ActorStatusKind {
        self.actor.status()
    }
}
