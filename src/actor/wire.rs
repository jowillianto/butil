/*
 * This will serve to abstract away things allowing an actor
 * to communicate over the wire. The design of this specific wire
 * bearing system will be to allow this kind of communication
 *
 * - Request (can tx) -> RawRequest (cannot tx)
 * - Response (the tx) -> RawResponse (only the payload)
 *
 * Hence, there are 5 concepts used for the system.
 * - Request
 * - Response
 * - RawRequest
 * - RawResponse
 * - Id
 */

use super::timed_receiver::{TimedReceiver, TimedReceiverError};
use super::{Actor, ActorConfig, ActorStatusKind, Context};
use crate::{BoundedKeyVec, Worker, wait_or};
use std::marker::PhantomData;

/*
 * Id generator for request/response pairing. Only ever called from inside the
 * wire actor loop, so it takes `&mut self` and needs no interior mutability.
 */
pub trait GenId<Id> {
    fn gen_id(&mut self) -> Id;
}

/*
 * Request to RawRequest
 */
pub trait ToRawRequest<Raw, Id> {
    fn to_raw(&self, id: &Id) -> Raw;
}

/*
 * Getting Id
 */
pub trait GetId<Id> {
    fn get_id(&self) -> &Id;
}

/*
 * Wire implementation, this is usually a pubsub model but could be anything.
 */
pub trait AsyncWireOut<RawReq> {
    type Error;
    fn wire_out(&mut self, msg: RawReq) -> impl Send + Future<Output = Result<(), Self::Error>>;
}

/*
 * Handle an inbound raw request (the responder side), producing an optional raw
 * response to send back.
 */
pub trait AsyncOnReq<RawReq, RawRes> {
    fn on_req(&self, req: RawReq) -> impl Send + Future<Output = Option<RawRes>>;
}

/*
 * The requester actor context. Holds a pending map keyed by id, each entry a
 * tx channel back to the caller. `req` publishes and stores the tx; `res`
 * pairs an inbound raw response to its pending tx by id.
 */
pub struct AsyncWire<Id, G, RawRes, RawReq, Req, W>
where
    Id: Eq + Clone,
    G: GenId<Id>,
    RawRes: GetId<Id>,
    Req: ToRawRequest<RawReq, Id>,
    W: AsyncWireOut<RawReq>,
{
    reqs: BoundedKeyVec<
        Id,
        (
            tokio::time::Instant,
            tokio::sync::oneshot::Sender<Result<RawRes, WireError<W::Error>>>,
        ),
    >,
    id_gen: G,
    wire: W,
    cleanup_threshold: tokio::time::Duration,
    __t: PhantomData<(RawReq, Req)>,
}

impl<Id, G, RawRes, RawReq, Req, W> AsyncWire<Id, G, RawRes, RawReq, Req, W>
where
    Id: Eq + Clone,
    G: GenId<Id>,
    RawRes: GetId<Id>,
    Req: ToRawRequest<RawReq, Id>,
    W: AsyncWireOut<RawReq>,
{
    pub fn new(
        max_size: usize,
        cleanup_threshold: tokio::time::Duration,
        id_gen: G,
        wire: W,
    ) -> Self {
        Self {
            reqs: BoundedKeyVec::new(max_size),
            id_gen,
            wire,
            cleanup_threshold,
            __t: PhantomData,
        }
    }
}

pub enum WireEvent<RawReq, W: AsyncWireOut<RawReq>, Req, RawRes, Id> {
    Req {
        req: Req,
        tx: tokio::sync::oneshot::Sender<Result<RawRes, WireError<W::Error>>>,
    },
    Res {
        res: RawRes,
    },
    Fire {
        req: Req,
        tx: tokio::sync::oneshot::Sender<Result<(), WireError<W::Error>>>,
    },
    Drop {
        id: Id,
    },
    Cleanup,
}

impl<Id, G, RawRes, RawReq, Req, W> Context<WireEvent<RawReq, W, Req, RawRes, Id>>
    for AsyncWire<Id, G, RawRes, RawReq, Req, W>
where
    Id: Eq + Clone + Send + 'static,
    G: GenId<Id> + Send + 'static,
    RawRes: Send + GetId<Id> + 'static,
    RawReq: Send + 'static,
    Req: Send + ToRawRequest<RawReq, Id> + 'static,
    W: Send + AsyncWireOut<RawReq> + 'static,
    W::Error: Send + 'static,
{
    async fn on_event(&mut self, e: WireEvent<RawReq, W, Req, RawRes, Id>) -> bool {
        match e {
            WireEvent::Req { req, tx } => {
                let id = self.id_gen.gen_id();
                let raw = req.to_raw(&id);
                match self.wire.wire_out(raw).await {
                    Ok(()) => {
                        self.reqs
                            .insert_no_check(id, (tokio::time::Instant::now(), tx));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(WireError::Wire(e)));
                    }
                }
            }
            WireEvent::Fire { req, tx } => {
                let id = self.id_gen.gen_id();
                let raw = req.to_raw(&id);
                let _ = tx.send(self.wire.wire_out(raw).await.map_err(WireError::Wire));
            }
            WireEvent::Res { res } => {
                let id = res.get_id().clone();
                if let Some((_, tx)) = self.reqs.remove(&id) {
                    let _ = tx.send(Ok(res));
                }
            }
            WireEvent::Drop { id } => {
                self.reqs.remove(&id);
            }
            WireEvent::Cleanup => {
                let now = tokio::time::Instant::now();
                let threshold = self.cleanup_threshold;
                self.reqs
                    .filter_self(|(_, (inserted, _))| now.duration_since(*inserted) < threshold);
            }
        }
        true
    }
}

pub enum WireError<E> {
    Wire(E),
    Timeout,
    Closed,
}

/*
 * A cloneable handle onto a `WireActor`. Holds only the event sender and the
 * reply timeout; request ids are generated inside the actor loop, so a mailbox
 * needs no id generator and can be cloned freely to whatever issues requests.
 */
pub struct WireActorMailbox<Id, RawReq, RawRes, Req, W>
where
    W: AsyncWireOut<RawReq>,
{
    tx: tokio::sync::mpsc::Sender<WireEvent<RawReq, W, Req, RawRes, Id>>,
    timeout: tokio::time::Duration,
}

impl<Id, RawReq, RawRes, Req, W> Clone for WireActorMailbox<Id, RawReq, RawRes, Req, W>
where
    W: AsyncWireOut<RawReq>,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            timeout: self.timeout,
        }
    }
}

impl<Id, RawReq, RawRes, Req, W> WireActorMailbox<Id, RawReq, RawRes, Req, W>
where
    Id: Send + 'static,
    RawReq: Send + 'static,
    RawRes: Send + 'static,
    Req: Send + 'static,
    W: AsyncWireOut<RawReq> + Send + 'static,
    W::Error: Send + 'static,
{
    pub async fn req(&self, req: Req) -> Result<RawRes, WireError<W::Error>> {
        let (tx, rx) = TimedReceiver::new(self.timeout);
        if self.tx.send(WireEvent::Req { req, tx }).await.is_err() {
            return Err(WireError::Closed);
        }
        match rx.recv().await {
            Ok(result) => result,
            Err(TimedReceiverError::Timeout) => Err(WireError::Timeout),
            Err(TimedReceiverError::Drop) => Err(WireError::Closed),
        }
    }

    pub async fn res(&self, res: RawRes) -> bool {
        self.tx.send(WireEvent::Res { res }).await.is_ok()
    }

    pub async fn fire(&self, req: Req) -> Result<(), WireError<W::Error>> {
        let (tx, rx) = TimedReceiver::new(self.timeout);
        if self.tx.send(WireEvent::Fire { req, tx }).await.is_err() {
            return Err(WireError::Closed);
        }
        match rx.recv().await {
            Ok(result) => result,
            Err(TimedReceiverError::Timeout) => Err(WireError::Timeout),
            Err(TimedReceiverError::Drop) => Err(WireError::Closed),
        }
    }

    pub fn tx(&self) -> tokio::sync::mpsc::Sender<WireEvent<RawReq, W, Req, RawRes, Id>> {
        self.tx.clone()
    }
}

pub struct WireActor<Id, G, RawReq, RawRes, Req, W>
where
    Id: Eq + Clone + Send + Sync + 'static,
    G: GenId<Id> + Send + Sync + 'static,
    RawReq: Send + Sync + 'static,
    RawRes: GetId<Id> + Send + Sync + 'static,
    Req: Send + Sync + 'static,
    W: AsyncWireOut<RawReq> + Send + Sync + 'static,
    W::Error: Send + 'static,
{
    actor: Actor<WireEvent<RawReq, W, Req, RawRes, Id>>,
    mailbox: WireActorMailbox<Id, RawReq, RawRes, Req, W>,
    cleanup: Worker<()>,
    __g: PhantomData<G>,
}

impl<Id, G, RawReq, RawRes, Req, W> WireActor<Id, G, RawReq, RawRes, Req, W>
where
    Id: Eq + Clone + Send + Sync + 'static,
    G: GenId<Id> + Send + Sync + 'static,
    RawReq: Send + Sync + 'static,
    RawRes: GetId<Id> + Send + Sync + 'static,
    Req: Send + Sync + 'static,
    W: Send + Sync + AsyncWireOut<RawReq> + 'static,
    W::Error: Send + 'static,
{
    pub fn new(
        config: ActorConfig,
        buf_size: usize,
        max_size: usize,
        timeout: tokio::time::Duration,
        cleanup_threshold: tokio::time::Duration,
        id_gen: G,
        wire: W,
    ) -> Self
    where
        Req: ToRawRequest<RawReq, Id>,
    {
        let (actor, tx) = Actor::new_bounded(
            config,
            buf_size,
            AsyncWire::new(max_size, cleanup_threshold, id_gen, wire),
        );
        let cleanup_tx = tx.clone();
        let cleanup = Worker::new(async move |cancel_token| {
            let mut interval = tokio::time::interval(cleanup_threshold);
            interval.tick().await;
            while wait_or(interval.tick(), cancel_token.cancelled())
                .await
                .is_some()
            {
                if cleanup_tx.send(WireEvent::Cleanup).await.is_err() {
                    break;
                }
            }
        });
        Self {
            actor,
            mailbox: WireActorMailbox { tx, timeout },
            cleanup,
            __g: PhantomData,
        }
    }

    pub fn mailbox(&self) -> WireActorMailbox<Id, RawReq, RawRes, Req, W> {
        self.mailbox.clone()
    }

    pub async fn req(&self, req: Req) -> Result<RawRes, WireError<W::Error>> {
        self.mailbox.req(req).await
    }

    pub async fn res(&self, res: RawRes) -> bool {
        self.mailbox.res(res).await
    }

    pub async fn fire(&self, req: Req) -> Result<(), WireError<W::Error>> {
        self.mailbox.fire(req).await
    }

    pub fn tx(&self) -> tokio::sync::mpsc::Sender<WireEvent<RawReq, W, Req, RawRes, Id>> {
        self.mailbox.tx()
    }

    pub async fn stop(&self) {
        self.cleanup.cancel();
        self.actor.stop().await;
    }

    pub async fn wait(&self) {
        self.actor.wait().await;
    }

    pub fn status(&self) -> ActorStatusKind {
        self.actor.status()
    }
}

/*
 * Auto implemented GenId
 */
pub struct UuidV4Gen;

impl GenId<uuid::Uuid> for UuidV4Gen {
    fn gen_id(&mut self) -> uuid::Uuid {
        uuid::Uuid::new_v4()
    }
}

pub struct Uint64Gen {
    inner: u64,
}

impl Uint64Gen {
    pub fn new() -> Self {
        Self { inner: 0 }
    }
}

impl Default for Uint64Gen {
    fn default() -> Self {
        Self::new()
    }
}

impl GenId<u64> for Uint64Gen {
    fn gen_id(&mut self) -> u64 {
        let id = self.inner;
        self.inner = self.inner.wrapping_add(1);
        id
    }
}
