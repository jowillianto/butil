use crate::actor::prelude::Lifecycle;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

/// Oneof
/// Init -> not started
/// Active -> started
/// Stopping -> a stop command have been issued but have not stopped
/// ShutdownGraceful -> gracefully shutdown
/// ShutdownForce -> forcefully shutdown
#[atomic_enum::atomic_enum]
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
        self.inner.load(std::sync::atomic::Ordering::Acquire)
    }

    /// make the actor status active
    pub fn activate(&self) {
        let _ = self.inner.compare_exchange(
            ActorStatusKind::Init,
            ActorStatusKind::Active,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        );
    }

    /// Set to stop, allows any transfer
    pub fn stop(&self) {
        let _ = self.inner.compare_exchange(
            ActorStatusKind::Active,
            ActorStatusKind::Stopping,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
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
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
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
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
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

pub struct ActorConfig {
    pub shutdown_action: ShutdownAction,
    pub cancel_token: Option<CancellationToken>,
}

impl Default for ActorConfig {
    fn default() -> Self {
        Self {
            shutdown_action: ShutdownAction::Drain,
            cancel_token: None,
        }
    }
}

pub async fn actor_loop<
    E: 'static + Send,
    L: Lifecycle<E>,
    S: tokio_stream::Stream<Item = E> + Unpin,
>(
    action: ShutdownAction,
    mut lifecyle: L,
    mut stream: S,
    status: ActorStatus,
    cancel_token: CancellationToken,
) {
    /*
     * Lifecycle init
     */
    lifecyle.init().await;
    /*
     * Lifecycle run
     */
    status.activate();
    let mut should_drain = true;
    while let Some(e) = crate::wait_or_option(stream.next(), cancel_token.cancelled()).await {
        if !lifecyle.on_event(e).await {
            should_drain = false;
            break;
        }
    }
    if should_drain {
        if action == ShutdownAction::Drain {
            while let Some(Some(e)) = futures::FutureExt::now_or_never(stream.next()) {
                if !lifecyle.on_event(e).await {
                    break;
                }
            }
        } else if action == ShutdownAction::Wait {
            while !lifecyle.is_complete()
                && let Some(e) = stream.next().await
            {
                if !lifecyle.on_event(e).await {
                    break;
                }
            }
        }
    }
    lifecyle.deinit().await;
    if lifecyle.is_complete() {
        status.shutdown_graceful();
    } else {
        status.shutdown_force();
    }
}

/*
 * Actors should be accompanied by their mailboxes, so add a mailbox associated with
 * the actor over there.
 */
pub struct Actor {
    worker: tokio::sync::Mutex<crate::Worker<()>>,
    status: ActorStatus,
}

impl Actor {
    pub fn new<
        E: 'static + Send,
        L: 'static + Send + Lifecycle<E>,
        S: 'static + tokio_stream::Stream<Item = E> + Send + Unpin,
    >(
        config: ActorConfig,
        ctx: L,
        stream: S,
    ) -> Self {
        let status = ActorStatus::new();
        let status2 = status.clone();
        let action = config.shutdown_action;
        let mut arg = crate::WorkerArg::new(async move |cancel_token| {
            actor_loop(action, ctx, stream, status2, cancel_token).await
        });
        if let Some(cancel_token) = config.cancel_token {
            arg = arg.with_cancel_token(cancel_token);
        }
        Self {
            worker: tokio::sync::Mutex::new(arg.spawn()),
            status,
        }
    }
    pub fn new_bounded<E: 'static + Send, L: 'static + Send + Lifecycle<E>>(
        config: ActorConfig,
        buf_size: usize,
        ctx: L,
    ) -> (Self, tokio::sync::mpsc::Sender<E>) {
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
