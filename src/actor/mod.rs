pub mod record;
#[cfg(feature = "actor-redis")]
pub mod redis;
pub mod wire;
use std::sync::{Arc, atomic::Ordering};

use atomic_enum::atomic_enum;

use crate::{KeyVec, Worker, timer::Either, wait_or_option};

/* Receiver */
pub enum Error {
    Timeout,
    Drop,
}
impl Error {
    pub fn is_timeout(&self) -> bool {
        matches!(self, &Self::Timeout)
    }
    pub fn is_dropped(&self) -> bool {
        matches!(self, &Self::Drop)
    }
}

pub struct RecvCtx {
    beg_noti: tokio::sync::Notify,
    end_noti: tokio::sync::Notify,
}

impl RecvCtx {
    pub fn start_countdown(&self) {
        self.beg_noti.notify_one();
    }
    pub fn end_countdown(&self) {
        self.end_noti.notify_one();
    }
    pub async fn wait_start(&self) {
        self.beg_noti.notified().await;
    }
    pub async fn wait_end(&self) {
        self.end_noti.notified().await;
    }
    pub fn new(dur: tokio::time::Duration) -> (Worker<()>, Arc<RecvCtx>) {
        let ctx = Arc::new(Self {
            beg_noti: tokio::sync::Notify::new(),
            end_noti: tokio::sync::Notify::new(),
        });
        let ctx2 = ctx.clone();
        let worker = Worker::new(async move |tok| {
            ctx2.wait_start().await;
            Either::wait(tokio::time::sleep(dur), tok.cancelled()).await;
            ctx2.end_countdown();
        });
        (worker, ctx)
    }
}

pub struct Receiver<T> {
    inner: tokio::sync::oneshot::Receiver<T>,
    ctx: Arc<RecvCtx>,
    _worker: Worker<()>,
}

impl<T> Receiver<T> {
    pub async fn recv(self) -> Result<T, Error> {
        self.ctx.start_countdown();
        match Either::wait(self.ctx.wait_end(), self.inner).await {
            Either::Left(_) => Err(Error::Timeout),
            Either::Right(Ok(v)) => Ok(v),
            Either::Right(Err(_)) => Err(Error::Drop),
        }
    }
}

pub fn new_pair<T>(
    timeout: tokio::time::Duration,
) -> (tokio::sync::oneshot::Sender<T>, Receiver<T>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let (worker, ctx) = RecvCtx::new(timeout);
    let rx = Receiver {
        inner: rx,
        ctx,
        _worker: worker,
    };
    (tx, rx)
}

/* Actor System */

/* Lifecycle phase shared across long-lived services. Wrapped by the
 * `atomic_enum` macro so a service can publish its current phase to
 * external observers without taking a lock. */
#[atomic_enum]
#[derive(PartialEq, Eq)]
pub enum ActorStatus {
    Init,
    Active,
    Stopping,
    ShutdownGraceful,
    ShutdownForce,
}

/* Cloneable handle to a shared `ActorStatus`. Transitions are CAS-only
 * and idempotent: an invalid transition is a no-op rather than an error.
 *
 * Allowed edges:
 *   Init     -> Active            (activate)
 *   Active   -> Stopping          (stop)
 *   Active   -> ShutdownGraceful  (shutdown_graceful)
 *   Stopping -> ShutdownGraceful  (shutdown_graceful)
 *   Active   -> ShutdownForce     (shutdown_force)
 *   Stopping -> ShutdownForce     (shutdown_force)
 */
#[derive(Debug, Clone)]
pub struct Status {
    inner: Arc<AtomicActorStatus>,
}

impl Status {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicActorStatus::new(ActorStatus::Init)),
        }
    }

    pub fn phase(&self) -> ActorStatus {
        self.inner.load(Ordering::Acquire)
    }

    pub fn activate(&self) {
        let _ = self.inner.compare_exchange(
            ActorStatus::Init,
            ActorStatus::Active,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub fn stop(&self) {
        let _ = self.inner.compare_exchange(
            ActorStatus::Active,
            ActorStatus::Stopping,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub fn shutdown_graceful(&self) {
        let current = ActorStatus::Active;
        while self
            .inner
            .compare_exchange_weak(
                current,
                ActorStatus::ShutdownGraceful,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            if current == ActorStatus::Init
                || current == ActorStatus::ShutdownGraceful
                || current == ActorStatus::ShutdownForce
            {
                break;
            }
        }
    }
    pub fn shutdown_force(&self) {
        let current = ActorStatus::Active;
        while self
            .inner
            .compare_exchange_weak(
                current,
                ActorStatus::ShutdownForce,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            if current == ActorStatus::Init
                || current == ActorStatus::ShutdownGraceful
                || current == ActorStatus::ShutdownForce
            {
                break;
            }
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum ShutdownAction {
    Force, /* Handles the current event being handled and kill the event loop */
    Drain, /* Handles all events that can be handled and kills the loop */
    Wait,  /* Handles events until Context::is_complete is true */
}

pub trait Context<E: Send + 'static>: Send {
    fn init(&mut self) -> impl Send + Future<Output = ()> {
        async {}
    }
    /* Return `true` to keep the loop running; `false` to terminate (deinit
     * still runs afterwards). */
    fn on_event(&mut self, e: E) -> impl Send + Future<Output = bool> {
        async {
            let _ = e;
            true
        }
    }
    fn deinit(&mut self) -> impl Send + Future<Output = ()> {
        async {}
    }
    fn is_complete(&self) -> bool {
        true
    }
}

pub struct ActorConfig {
    pub shutdown_action: ShutdownAction,
}

#[derive(Debug)]
pub struct Actor<E: Send + 'static> {
    worker: tokio::sync::Mutex<Worker<()>>,
    status: Status,
    event: std::marker::PhantomData<E>,
}

impl<E: Send + 'static> Actor<E> {
    pub fn new<C, S>(config: ActorConfig, ctx: C, stream: S) -> Self
    where
        C: 'static + Context<E>,
        S: 'static + tokio_stream::Stream<Item = E> + Send + Unpin,
    {
        use tokio_stream::StreamExt;
        let mut ctx = ctx;
        let mut stream = stream;
        let status = Status::new();
        let status2 = status.clone();
        let worker = Worker::new(async move |cancel_token| {
            ctx.init().await;
            status2.activate();
            let mut should_drain = true;
            while let Some(e) = wait_or_option(stream.next(), cancel_token.cancelled()).await {
                if !ctx.on_event(e).await {
                    should_drain = false;
                    break;
                }
            }
            if should_drain {
                if config.shutdown_action == ShutdownAction::Drain {
                    use futures::FutureExt;
                    while let Some(Some(e)) = stream.next().now_or_never() {
                        if !ctx.on_event(e).await {
                            break;
                        }
                    }
                } else if config.shutdown_action == ShutdownAction::Wait {
                    while !ctx.is_complete()
                        && let Some(e) = stream.next().await
                    {
                        if !ctx.on_event(e).await {
                            break;
                        }
                    }
                }
            }
            ctx.deinit().await;
            if ctx.is_complete() {
                status2.shutdown_graceful();
            } else {
                status2.shutdown_force();
            }
        });
        Self {
            worker: tokio::sync::Mutex::new(worker),
            status,
            event: std::marker::PhantomData,
        }
    }

    pub fn new_bounded<C>(
        config: ActorConfig,
        buf_size: usize,
        ctx: C,
    ) -> (Self, tokio::sync::mpsc::Sender<E>)
    where
        C: 'static + Context<E>,
    {
        let (tx, rx) = tokio::sync::mpsc::channel::<E>(buf_size);
        let actor = Self::new(
            config,
            ctx,
            tokio_stream::wrappers::ReceiverStream::new(rx),
        );
        (actor, tx)
    }

    pub async fn stop(&self) {
        self.status.stop();
        self.worker.lock().await.cancel();
    }
    pub async fn wait(&self) {
        self.worker.lock().await.wait().await;
    }
    pub fn status(&self) -> ActorStatus {
        self.status.phase()
    }
}

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
    pub fn len(timeout: tokio::time::Duration) -> (Self, Receiver<usize>) {
        let (tx, rx) = new_pair(timeout);
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
