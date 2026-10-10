use std::sync::{Arc, atomic::AtomicU64};

use super::{ActorArg, ActorStatus, ActorStatusKind, oneshot};
use crate::{
    actor::prelude::{ActorCtl, GetMailbox, Lifecycle},
    collections::BoundedMap,
};

pub enum Error<E> {
    Timeout,
    Drop,
    Closed,
    Send(E),
}

pub trait GenId: Clone {
    type Id;
    fn gen_id(&self) -> Self::Id;
}

pub trait GetId {
    type Id;
    fn get_id(&self) -> Self::Id;
}
pub trait ToSerializable {
    type S: GetId;
    fn to_serializable(&self, id: &<Self::S as GetId>::Id) -> Self::S;
}

pub trait SendSerialized<S> {
    type E;
    fn send_serialized(&self, s: S) -> impl Send + Future<Output = Result<(), Self::E>>;
}

enum Event<Req: GetId, Res: GetId, E> {
    Req {
        req: Req,
        tx: tokio::sync::oneshot::Sender<Result<Res, E>>,
    },
    Res {
        res: Res,
    },
    Refresh,
}

struct Context<Res: GetId, T, E> {
    pending_requests: BoundedMap<Res::Id, tokio::sync::oneshot::Sender<Result<Res, E>>>,
    sender: T,
}

impl<
    Id: PartialEq + Send,
    Req: 'static + Send + GetId<Id = Id>,
    Res: 'static + Send + GetId<Id = Id>,
    E: 'static + Send,
    T: Send + Sync + SendSerialized<Req, E = E>,
> Lifecycle<Event<Req, Res, E>> for Context<Res, T, E>
{
    async fn on_event(&mut self, e: Event<Req, Res, E>) -> bool {
        match e {
            Event::Req { req, tx } => {
                let id = req.get_id();
                match self.sender.send_serialized(req).await {
                    Ok(_) => {
                        self.pending_requests.insert(id, tx);
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            }
            Event::Res { res } => {
                let id = res.get_id();
                if let Some(tx) = self.pending_requests.remove(&id) {
                    let _ = tx.send(Ok(res));
                }
            }
            Event::Refresh => {
                self.pending_requests.filter_self(|(_, tx)| !tx.is_closed());
            }
        }
        true
    }
}

pub struct Mailbox<Id, RawReq: GetId<Id = Id>, Res: GetId<Id = Id>, G: GenId<Id = Id>, E> {
    tx: tokio::sync::mpsc::Sender<Event<RawReq, Res, E>>,
    generator: G,
}

impl<Id, RawReq: GetId<Id = Id>, Res: GetId<Id = Id>, G: GenId<Id = Id>, E> Clone
    for Mailbox<Id, RawReq, Res, G, E>
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            generator: self.generator.clone(),
        }
    }
}

impl<Id, RawReq: GetId<Id = Id>, Res: GetId<Id = Id>, G: GenId<Id = Id>, E>
    Mailbox<Id, RawReq, Res, G, E>
{
    pub async fn req<Req: ToSerializable<S = RawReq>>(&self, req: Req) -> Result<Res, Error<E>> {
        let id = self.generator.gen_id();
        let raw_req = req.to_serializable(&id);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Res, E>>();
        match self.tx.send(Event::Req { req: raw_req, tx }).await {
            Ok(_) => (),
            Err(_) => return Err(Error::Closed),
        }
        match rx.await {
            Ok(res) => res.map_err(Error::Send),
            Err(_) => Err(Error::Drop),
        }
    }
    pub async fn req_with_timeout<Req: ToSerializable<S = RawReq>>(
        &self,
        req: Req,
        timeout: tokio::time::Duration,
    ) -> Result<Res, Error<E>> {
        let id = self.generator.gen_id();
        let raw_req = req.to_serializable(&id);
        let (tx, rx) = oneshot::oneshot::<Result<Res, E>>(timeout);
        match self.tx.send(Event::Req { req: raw_req, tx }).await {
            Ok(_) => (),
            Err(_) => return Err(Error::Closed),
        }
        match rx.recv().await {
            Ok(res) => res.map_err(Error::Send),
            Err(e) => {
                if e.is_timeout() {
                    return Err(Error::Timeout);
                } else {
                    return Err(Error::Drop);
                }
            }
        }
    }
    pub async fn res(&self, res: Res) -> Result<(), Error<E>> {
        match self.tx.send(Event::Res { res }).await {
            Ok(_) => Ok(()),
            Err(_) => Err(Error::Closed),
        }
    }
}

pub struct Service<Id, RawReq: GetId<Id = Id>, Res: GetId<Id = Id>, G: GenId<Id = Id>, E> {
    worker: tokio::sync::Mutex<crate::async_utils::Worker<()>>,
    _refresh: crate::async_utils::Worker<()>,
    status: ActorStatus,
    mailbox: Mailbox<Id, RawReq, Res, G, E>,
}

impl<
    Id: 'static + PartialEq + Send,
    RawReq: 'static + Send + GetId<Id = Id>,
    Res: 'static + Send + GetId<Id = Id>,
    G: GenId<Id = Id>,
    E: 'static + Send,
> Service<Id, RawReq, Res, G, E>
{
    pub fn new<T: 'static + Send + Sync + SendSerialized<RawReq, E = E>>(
        config: ActorArg,
        buf_size: usize,
        max_pending: usize,
        refresh_interval: tokio::time::Duration,
        sender: T,
        generator: G,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(buf_size);
        let (worker, status) = config.run_with_lifecycle(
            Context {
                pending_requests: BoundedMap::new(max_pending),
                sender,
            },
            tokio_stream::wrappers::ReceiverStream::new(rx),
        );
        let refresh_tx = tx.clone();
        let refresh = crate::async_utils::WorkerArg::new(async move |cancel_token| {
            let mut interval = tokio::time::interval(refresh_interval);
            interval.tick().await;
            while crate::async_utils::wait_or(interval.tick(), cancel_token.cancelled())
                .await
                .is_some()
            {
                if refresh_tx.send(Event::Refresh).await.is_err() {
                    break;
                }
            }
        })
        .spawn();
        Self {
            worker: tokio::sync::Mutex::new(worker),
            _refresh: refresh,
            status,
            mailbox: Mailbox { tx, generator },
        }
    }
}

impl<Id, RawReq: GetId<Id = Id>, Res: GetId<Id = Id>, G: GenId<Id = Id>, E> GetMailbox
    for Service<Id, RawReq, Res, G, E>
{
    type M = Mailbox<Id, RawReq, Res, G, E>;
    fn get_mailbox(&self) -> Self::M {
        self.mailbox.clone()
    }
}

#[async_trait::async_trait]
impl<
    Id: 'static,
    RawReq: 'static + Send + GetId<Id = Id>,
    Res: 'static + Send + GetId<Id = Id>,
    G: 'static + Send + Sync + GenId<Id = Id>,
    E: 'static + Send,
> ActorCtl for Service<Id, RawReq, Res, G, E>
{
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

#[derive(Clone, Default)]
pub struct Uint64Gen(Arc<AtomicU64>);

impl GenId for Uint64Gen {
    type Id = u64;
    fn gen_id(&self) -> u64 {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Clone, Default)]
pub struct UuidGen;

impl GenId for UuidGen {
    type Id = uuid::Uuid;
    fn gen_id(&self) -> uuid::Uuid {
        uuid::Uuid::new_v4()
    }
}
