use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};

use atomic_enum::atomic_enum;
use tokio_util::sync::CancellationToken;

use crate::{Worker, wait_or_option};

/// Oneof
/// Init -> not started
/// Active -> started
/// Stopping -> a stop command have been issued but have not stopped
/// ShutdownGraceful -> gracefully shutdown
/// ShutdownForce -> forcefully shutdown
#[atomic_enum]
#[derive(PartialEq, Eq)]
pub enum ActorStatusKind {
    Init,
    Active,
    Stopping,
    ShutdownGraceful,
    ShutdownForce,
}

/// Cloneable version of the
#[derive(Debug, Clone)]
pub struct ActorStatus {
    inner: Arc<AtomicActorStatusKind>,
}

impl ActorStatus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicActorStatusKind::new(ActorStatusKind::Init)),
        }
    }

    /// get the current phase
    pub fn phase(&self) -> ActorStatusKind {
        self.inner.load(Ordering::Acquire)
    }

    /// make the actor status active
    pub fn activate(&self) {
        let _ = self.inner.compare_exchange(
            ActorStatusKind::Init,
            ActorStatusKind::Active,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Set to stop, allows any transfer
    pub fn stop(&self) {
        let _ = self.inner.compare_exchange(
            ActorStatusKind::Active,
            ActorStatusKind::Stopping,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// forces a shutdown only allows a change from active or stopping
    /// and is a no-op otherwise
    pub fn shutdown_graceful(&self) {
        let current = ActorStatusKind::Active;
        while self
            .inner
            .compare_exchange_weak(
                current,
                ActorStatusKind::ShutdownGraceful,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            if current == ActorStatusKind::Init
                || current == ActorStatusKind::ShutdownGraceful
                || current == ActorStatusKind::ShutdownForce
            {
                break;
            }
        }
    }
    /// forces a shutdown only allows a change from active or stopping
    /// and is no-op otherwise
    pub fn shutdown_force(&self) {
        let current = ActorStatusKind::Active;
        while self
            .inner
            .compare_exchange_weak(
                current,
                ActorStatusKind::ShutdownForce,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            if current == ActorStatusKind::Init
                || current == ActorStatusKind::ShutdownGraceful
                || current == ActorStatusKind::ShutdownForce
            {
                break;
            }
        }
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
    status: ActorStatus,
    event: std::marker::PhantomData<E>,
}

impl<E: Send + 'static> Actor<E> {
    pub fn new<C, S>(config: ActorConfig, ctx: C, stream: S) -> Self
    where
        C: 'static + Context<E> + Send,
        S: 'static + tokio_stream::Stream<Item = E> + Send + Unpin,
    {
        use tokio_stream::StreamExt;
        let mut ctx = ctx;
        let mut stream = stream;
        let status = ActorStatus::new();
        let status2 = status.clone();
        let f = async move |cancel_token: CancellationToken| {
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
        };
        Self {
            worker: tokio::sync::Mutex::new(Worker::new(f)),
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
        let actor = Self::new(config, ctx, tokio_stream::wrappers::ReceiverStream::new(rx));
        (actor, tx)
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

pub trait ActorInfo: Send + Sync {
    const NAME: &'static str;
    fn status(&self) -> ActorStatusKind;
}

#[async_trait::async_trait]
impl<E: Send + Sync + 'static> ActorCtl for Actor<E> {
    async fn stop(&self) {
        Actor::stop(self).await;
    }
    async fn wait(&self) {
        Actor::wait(self).await;
    }
}

#[async_trait::async_trait]
pub trait ActorCtl: Send + Sync {
    async fn stop(&self);
    async fn wait(&self);
    async fn shutdown_and_wait(&self) {
        self.stop().await;
        self.wait().await;
    }
}

/// Everything the registry needs out of one actor, held as a single trait object.
/// `ActorInfo` carries an associated const so it cannot be a supertrait here.
pub trait ActorHandle: ActorCtl + Any {
    fn status(&self) -> ActorStatusKind;
}

impl<T: ActorInfo + ActorCtl + Any> ActorHandle for T {
    fn status(&self) -> ActorStatusKind {
        ActorInfo::status(self)
    }
}

pub struct ActorRegistryEntry {
    name: &'static str,
    handle: Arc<dyn ActorHandle>,
}

pub struct ActorRegistry {
    inner: HashMap<TypeId, ActorRegistryEntry>,
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
    /// Registers an actor under its own type, handing it back when the type is taken.
    pub fn register<T: ActorInfo + ActorCtl + Any>(&mut self, a: T) -> Option<T> {
        if self.inner.contains_key(&TypeId::of::<T>()) {
            return Some(a);
        }
        self.inner.insert(
            TypeId::of::<T>(),
            ActorRegistryEntry {
                name: T::NAME,
                handle: Arc::new(a),
            },
        );
        None
    }
    /// The actor registered under the type, `None` when it is absent.
    pub fn get_option<T: Any + ActorInfo + ActorCtl>(&self) -> Option<&T> {
        self.inner
            .get(&TypeId::of::<T>())
            .and_then(|entry| (entry.handle.as_ref() as &dyn Any).downcast_ref::<T>())
    }
    /// Every registered actor, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn ActorHandle> {
        self.inner.values().map(|entry| entry.handle.as_ref())
    }
    pub fn get<T: Any + ActorInfo + ActorCtl>(&self) -> &T {
        self.get_option::<T>()
            .unwrap_or_else(|| panic!("actor '{}' is not registered", T::NAME))
    }
    pub async fn stop<T: Any>(&self) -> bool {
        let Some(entry) = self.inner.get(&TypeId::of::<T>()) else {
            return false;
        };
        entry.handle.stop().await;
        true
    }
    pub async fn wait<T: Any>(&self) -> bool {
        let Some(entry) = self.inner.get(&TypeId::of::<T>()) else {
            return false;
        };
        entry.handle.wait().await;
        true
    }
    /// Stops the actor and waits for it, `false` when the type is absent.
    pub async fn shutdown_and_wait<T: Any>(&self) -> bool {
        let Some(entry) = self.inner.get(&TypeId::of::<T>()) else {
            return false;
        };
        entry.handle.shutdown_and_wait().await;
        true
    }
    /// Every registered type name with its current phase.
    pub fn list(&self) -> Vec<(&'static str, ActorStatusKind)> {
        self.inner
            .values()
            .map(|entry| (entry.name, entry.handle.status()))
            .collect()
    }
    pub async fn info<T: Any>(&self) -> ActorStatusKind {
        match self.inner.get(&TypeId::of::<T>()) {
            Some(entry) => entry.handle.status(),
            None => ActorStatusKind::ShutdownForce,
        }
    }
}

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
