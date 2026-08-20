pub mod actor;
pub mod pubsub;
pub mod record;
#[cfg(feature = "actor-redis")]
pub mod redis;
pub mod timed_receiver;
pub mod wire;
use std::sync::Arc;

use crate::KeyVec;
pub use actor::{
    Actor, ActorConfig, ActorCtl, ActorInfo, ActorRegistry, ActorStatus, ActorStatusKind, Context,
    ShutdownAction,
};
pub use timed_receiver::{TimedReceiver, TimedReceiverError};

/*
 * Listener based actor
 */
#[async_trait::async_trait]
pub trait ActorSender<E: Send + Sync + 'static>: Send + Sync {
    async fn send(&self, e: &E);
}

pub enum ListenerEvent<E: Send + Sync + 'static> {
    Reg {
        id: usize,
        tx: Arc<dyn ActorSender<E>>,
    },
    Unreg {
        id: usize,
    },
    Notify {
        event: E,
    },
    Len {
        tx: tokio::sync::oneshot::Sender<usize>,
    },
}

impl<E: Send + Sync + 'static> ListenerEvent<E> {
    pub fn reg(tx: impl 'static + ActorSender<E>) -> (Self, usize) {
        let tx = Arc::new(tx) as Arc<dyn ActorSender<E>>;
        let id = (tx.as_ref() as *const dyn ActorSender<E>).addr();
        (Self::Reg { id, tx }, id)
    }
    pub fn unreg(id: usize) -> Self {
        Self::Unreg { id }
    }
    pub fn len(timeout: tokio::time::Duration) -> (Self, timed_receiver::TimedReceiver<usize>) {
        let (tx, rx) = timed_receiver::TimedReceiver::new(timeout);
        (Self::Len { tx: tx }, rx)
    }
    pub fn notify(event: E) -> Self {
        Self::Notify { event }
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

pub struct ListenerCtx<E: Send + Sync + 'static> {
    listeners: KeyVec<usize, Arc<dyn ActorSender<E>>>,
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
                    tx.send(&event).await;
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
