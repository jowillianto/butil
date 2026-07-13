use super::{Actor, ActorConfig, ActorStatus, Context, new_pair};
use crate::KeyVec;
use crate::Worker;
use crate::wait_or;

pub trait Record<Id> {
    type Error;
    fn reg(
        &mut self,
        id: &Id,
        ttl: tokio::time::Duration,
    ) -> impl Send + Future<Output = Result<(), Self::Error>>;
    fn unreg(&mut self, id: &Id) -> impl Send + Future<Output = Result<(), Self::Error>>;
    fn has(&mut self, _id: &Id) -> impl Send + Future<Output = Result<bool, Self::Error>> {
        async { Ok(false) }
    }
}

pub struct RecordState<Id: Eq, R: Record<Id>> {
    ids: KeyVec<Id, tokio::time::Instant>,
    ttl: tokio::time::Duration,
    record: R,
}

impl<Id: Eq, R: Record<Id>> RecordState<Id, R> {
    pub fn new(ttl: tokio::time::Duration, record: R) -> Self {
        Self {
            ids: KeyVec::default(),
            ttl,
            record,
        }
    }
}

pub enum RecordEvent<Id> {
    Reg {
        id: Id,
    },
    Unreg {
        id: Id,
    },
    Has {
        id: Id,
        tx: tokio::sync::oneshot::Sender<bool>,
    },
    LocalHas {
        id: Id,
        tx: tokio::sync::oneshot::Sender<bool>,
    },
    Refresh,
}

impl<Id, R> Context<RecordEvent<Id>> for RecordState<Id, R>
where
    Id: Eq + Clone + Send + Sync + 'static,
    R: Record<Id> + Send + 'static,
    R::Error: Send + 'static,
{
    async fn on_event(&mut self, e: RecordEvent<Id>) -> bool {
        match e {
            RecordEvent::Reg { id } => {
                let now = tokio::time::Instant::now();
                let _ = self.record.reg(&id, self.ttl).await;
                match self.ids.get_mut(&id) {
                    Some(tp) => {
                        *tp = now;
                    }
                    None => {
                        self.ids.insert_no_check(id, now);
                    }
                }
            }
            RecordEvent::LocalHas { id, tx } => {
                let _ = tx.send(self.ids.get(&id).is_some());
            }
            RecordEvent::Has { id, tx } => {
                let _ = tx.send(self.record.has(&id).await.unwrap_or(false));
            }
            RecordEvent::Unreg { id } => {
                if self.ids.remove(&id).is_some() {
                    let _ = self.record.unreg(&id).await;
                }
            }
            RecordEvent::Refresh => {
                for (id, tp) in self.ids.iter_mut() {
                    let _ = self.record.reg(&id, self.ttl).await;
                    *tp = tokio::time::Instant::now();
                }
            }
        }
        true
    }

    async fn deinit(&mut self) {
        for id in self.ids.iter_keys() {
            let _ = self.record.unreg(&id).await;
        }
    }
}

pub struct RecordActor<Id: Eq + Clone + Send + Sync + 'static> {
    actor: Actor<RecordEvent<Id>>,
    tx: tokio::sync::mpsc::Sender<RecordEvent<Id>>,
    refresh_worker: Worker<()>,
}

impl<Id: Eq + Clone + Send + Sync + 'static> RecordActor<Id> {
    pub fn new<E: 'static + Send, R: Record<Id, Error = E> + Send + 'static>(
        config: ActorConfig,
        buf_size: usize,
        ttl: tokio::time::Duration,
        refresh_interval: tokio::time::Duration,
        record: R,
    ) -> Self {
        let (actor, tx) = Actor::new_bounded(config, buf_size, RecordState::new(ttl, record));
        let refresh_tx = tx.clone();
        let refresh_worker = Worker::new(async move |cancel_token| {
            let mut interval = tokio::time::interval(refresh_interval);
            while wait_or(interval.tick(), cancel_token.cancelled())
                .await
                .is_some()
            {
                if refresh_tx.send(RecordEvent::Refresh).await.is_err() {
                    break;
                }
            }
        });
        Self {
            actor,
            tx,
            refresh_worker,
        }
    }

    pub async fn reg(&self, id: Id) -> bool {
        self.tx.send(RecordEvent::Reg { id }).await.is_ok()
    }

    pub async fn unreg(&self, id: Id) -> bool {
        self.tx.send(RecordEvent::Unreg { id }).await.is_ok()
    }

    /* Is `id` registered on this instance? */
    pub async fn local_has(&self, id: Id) -> bool {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self
            .tx
            .send(RecordEvent::LocalHas { id, tx })
            .await
            .is_err()
        {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    /* Is `id` registered on any instance (per the backend)? `false` when the
     * backend only tracks locally. */
    pub async fn has(&self, id: Id) -> bool {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(RecordEvent::Has { id, tx }).await.is_err() {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    pub fn tx(&self) -> tokio::sync::mpsc::Sender<RecordEvent<Id>> {
        self.tx.clone()
    }

    pub fn mailbox(&self, timeout: tokio::time::Duration) -> RecordMailbox<Id> {
        RecordMailbox::new(self.tx.clone(), timeout)
    }

    pub async fn stop(&self) {
        self.actor.stop().await;
        self.refresh_worker.cancel();
    }

    pub async fn wait(&self) {
        self.actor.wait().await;
    }

    pub fn status(&self) -> ActorStatus {
        self.actor.status()
    }
}

/* A cloneable handle onto a `RecordActor`, for registering/querying presence
 * from anywhere that holds one. */
#[derive(Debug)]
pub struct RecordMailbox<Id> {
    tx: tokio::sync::mpsc::Sender<RecordEvent<Id>>,
    timeout: tokio::time::Duration,
}

impl<Id> Clone for RecordMailbox<Id> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            timeout: self.timeout,
        }
    }
}

impl<Id> RecordMailbox<Id> {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<RecordEvent<Id>>,
        timeout: tokio::time::Duration,
    ) -> Self {
        Self { tx, timeout }
    }

    pub async fn reg(&self, id: Id) -> bool {
        self.tx.send(RecordEvent::Reg { id }).await.is_ok()
    }

    pub async fn unreg(&self, id: Id) -> bool {
        self.tx.send(RecordEvent::Unreg { id }).await.is_ok()
    }

    pub async fn local_has(&self, id: Id) -> bool {
        let (tx, rx) = new_pair(self.timeout);
        if self
            .tx
            .send(RecordEvent::LocalHas { id, tx })
            .await
            .is_err()
        {
            return false;
        }
        rx.recv().await.unwrap_or(false)
    }

    pub async fn has(&self, id: Id) -> bool {
        let (tx, rx) = new_pair(self.timeout);
        if self.tx.send(RecordEvent::Has { id, tx }).await.is_err() {
            return false;
        }
        rx.recv().await.unwrap_or(false)
    }
}
