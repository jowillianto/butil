use std::sync::{Arc, atomic::AtomicU64};

use super::{ActorArg, ActorStatus, ActorStatusKind};
use crate::{
    actor::prelude::{GetMailbox, HandleEvent, Lifecycle},
    async_utils::wait_or,
    collections::LinearMap,
};

enum Event {
    Reg {
        sub_id: u64,
        worker: crate::async_utils::Worker<()>,
    },
    Unreg {
        sub_id: u64,
    },
}
struct ListenerCtx {
    subs: LinearMap<u64, crate::async_utils::Worker<()>>,
}

impl ListenerCtx {
    fn new() -> Self {
        Self {
            subs: LinearMap::default(),
        }
    }
}

impl Lifecycle<Event> for ListenerCtx {
    async fn on_event(&mut self, e: Event) -> bool {
        match e {
            Event::Reg { sub_id, worker } => {
                self.subs.insert_no_check(sub_id, worker);
            }
            Event::Unreg { sub_id } => {
                self.subs.remove(&sub_id);
            }
        }
        true
    }

    fn is_complete(&self) -> bool {
        false
    }
}

pub struct SubId {
    id: u64,
    act_tx: tokio::sync::mpsc::Sender<Event>,
}

impl SubId {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for SubId {
    fn drop(&mut self) {
        let _ = self.act_tx.try_send(Event::Unreg { sub_id: self.id });
    }
}

pub struct Actor<E: 'static + Send + Sync> {
    worker: tokio::sync::Mutex<crate::async_utils::Worker<()>>,
    status: ActorStatus,
    act_tx: tokio::sync::mpsc::Sender<Event>,
    tx: tokio::sync::broadcast::Sender<Arc<E>>,
    counter: Arc<AtomicU64>,
}

impl<E: 'static + Send + Sync> Actor<E> {
    pub fn new(config: ActorArg, buf_size: usize) -> Self {
        let (act_tx, act_rx) = tokio::sync::mpsc::channel(buf_size);
        let (worker, status) = config.run_with_lifecycle(
            ListenerCtx::new(),
            tokio_stream::wrappers::ReceiverStream::new(act_rx),
        );
        let (tx, _) = tokio::sync::broadcast::channel(buf_size);
        Self {
            worker: tokio::sync::Mutex::new(worker),
            status,
            act_tx,
            tx,
            counter: Arc::new(AtomicU64::new(0)),
        }
    }
    pub fn mailbox(&self) -> Mailbox<E> {
        Mailbox {
            tx: self.tx.clone(),
            act_tx: self.act_tx.clone(),
            counter: self.counter.clone(),
        }
    }
    pub async fn stop(&self) {
        self.status.stop();
        self.worker.lock().await.cancel();
    }
    pub async fn wait(&self) {
        self.worker.lock().await.wait().await;
    }
    pub fn status(&self) -> ActorStatusKind {
        self.status.phase()
    }
}

impl<E: 'static + Send + Sync> GetMailbox for Actor<E> {
    type M = Mailbox<E>;
    fn get_mailbox(&self) -> Self::M {
        self.mailbox()
    }
}

pub struct Mailbox<E: 'static + Send + Sync> {
    tx: tokio::sync::broadcast::Sender<Arc<E>>,
    act_tx: tokio::sync::mpsc::Sender<Event>,
    counter: Arc<AtomicU64>,
}

impl<E: 'static + Send + Sync> Clone for Mailbox<E> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            act_tx: self.act_tx.clone(),
            counter: self.counter.clone(),
        }
    }
}

impl<E: 'static + Send + Sync> Mailbox<E> {
    pub async fn sub<H: 'static + HandleEvent<Arc<E>>>(&self, mut handler: H) -> SubId {
        let sub_id = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut rx = self.tx.subscribe();
        let worker = crate::async_utils::WorkerArg::new(async move |app_token| {
            while let Some(e) = wait_or(rx.recv(), app_token.cancelled()).await {
                match e {
                    Ok(e) => handler.handle_event(e).await,
                    Err(e) => match e {
                        tokio::sync::broadcast::error::RecvError::Closed => break,
                        tokio::sync::broadcast::error::RecvError::Lagged(_) => continue,
                    },
                }
            }
        })
        .spawn();
        let _ = self.act_tx.send(Event::Reg { sub_id, worker }).await;
        SubId {
            id: sub_id,
            act_tx: self.act_tx.clone(),
        }
    }
    pub async fn unsub(&self, sub_id: u64) -> bool {
        self.act_tx.send(Event::Unreg { sub_id }).await.is_ok()
    }
    pub fn notify(&self, e: E) -> bool {
        self.tx.send(Arc::new(e)).is_ok()
    }
}
