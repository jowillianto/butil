use super::ctx::{RedisSubCtx, RegistryCtx};
use super::error::BridgeError;
use super::event::{InstanceBridgeMailbox, RedisSender, RegistryEvent};
use super::message;
use super::prelude::{FromProto, GetChannelName, InstanceId, OnRequest, ToProto};
use crate::actor::prelude::{ActorCtl, GetMailbox};
use crate::actor::{ActorArg, ActorStatus, ActorStatusKind, ShutdownAction, serialized};

pub struct InstanceService<Req: 'static + Send, Res: 'static + Send, C: GetChannelName> {
    worker: tokio::sync::Mutex<crate::async_utils::Worker<()>>,
    status: ActorStatus,
    registry_worker: tokio::sync::Mutex<crate::async_utils::Worker<()>>,
    _refresh: crate::async_utils::Worker<()>,
    pairing: serialized::Service<
        u64,
        message::InstanceMessage<Req, Res>,
        message::InstanceMessage<Req, Res>,
        serialized::Uint64Gen,
        redis::RedisError,
    >,
    mailbox: InstanceBridgeMailbox<Req, Res, C>,
    _sink: redis::aio::PubSubSink,
}

impl<
    Req: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Req as ToProto>::Msg>,
    Res: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Res as ToProto>::Msg>,
    C: 'static + Send + Sync + GetChannelName,
> InstanceService<Req, Res, C>
{
    pub async fn new<H: 'static + Send + Sync + Clone + OnRequest<Req, Res, E = BridgeError>>(
        client: redis::Client,
        instance_id: InstanceId,
        namer: C,
        handler: H,
        queue_size: usize,
        max_pending: usize,
        refresh_interval: tokio::time::Duration,
        handler_timeout: tokio::time::Duration,
        ttl: tokio::time::Duration,
    ) -> Result<Self, redis::RedisError> {
        let conn = client.get_connection_manager().await?;
        let (mut sink, stream) = client.get_async_pubsub().await?.split();
        sink.subscribe(namer.get_channel_name(instance_id)).await?;
        let sender = RedisSender {
            conn: conn.clone(),
            namer: namer.clone(),
        };
        let pairing = serialized::Service::new(
            ActorArg {
                shutdown_action: ShutdownAction::Drain,
                ..Default::default()
            },
            queue_size,
            max_pending,
            refresh_interval,
            sender.clone(),
            serialized::Uint64Gen::default(),
        );
        let mbox = pairing.get_mailbox();
        let (registry_tx, registry_rx) = tokio::sync::mpsc::channel(queue_size);
        let (registry_worker, _) = ActorArg {
            shutdown_action: ShutdownAction::Drain,
            ..Default::default()
        }
        .run_with_lifecycle(
            RegistryCtx {
                instance_id,
                conn,
                ttl,
                handler: handler.clone(),
                handler_timeout,
                mbox: mbox.clone(),
                owned: std::collections::HashSet::new(),
            },
            tokio_stream::wrappers::ReceiverStream::new(registry_rx),
        );
        let refresh_tx = registry_tx.clone();
        let refresh = crate::async_utils::WorkerArg::new(async move |cancel_token| {
            let mut interval =
                tokio::time::interval((ttl / 3).max(tokio::time::Duration::from_millis(1)));
            interval.tick().await;
            while crate::async_utils::wait_or(interval.tick(), cancel_token.cancelled())
                .await
                .is_some()
            {
                if refresh_tx.send(RegistryEvent::Refresh).await.is_err() {
                    break;
                }
            }
        })
        .spawn();
        let (worker, status) = ActorArg {
            shutdown_action: ShutdownAction::Drain,
            ..Default::default()
        }
        .run_with_lifecycle(
            RedisSubCtx {
                instance_id,
                sender,
                mbox,
                handler,
                handler_timeout,
            },
            stream,
        );
        Ok(Self {
            worker: tokio::sync::Mutex::new(worker),
            status,
            registry_worker: tokio::sync::Mutex::new(registry_worker),
            _refresh: refresh,
            pairing,
            mailbox: InstanceBridgeMailbox {
                namer,
                registry: registry_tx,
            },
            _sink: sink,
        })
    }
}

impl<Req: 'static + Send, Res: 'static + Send, C: GetChannelName> InstanceService<Req, Res, C> {
    pub fn mailbox(&self) -> InstanceBridgeMailbox<Req, Res, C> {
        self.mailbox.clone()
    }
}

impl<Req: 'static + Send, Res: 'static + Send, C: GetChannelName> GetMailbox
    for InstanceService<Req, Res, C>
{
    type M = InstanceBridgeMailbox<Req, Res, C>;
    fn get_mailbox(&self) -> Self::M {
        self.mailbox.clone()
    }
}

#[async_trait::async_trait]
impl<Req: 'static + Send, Res: 'static + Send, C: 'static + Send + Sync + GetChannelName> ActorCtl
    for InstanceService<Req, Res, C>
{
    fn status(&self) -> ActorStatusKind {
        self.status.phase()
    }
    async fn stop(&self) {
        self.status.stop();
        self.worker.lock().await.cancel();
        self._refresh.cancel();
        self.registry_worker.lock().await.cancel();
        self.pairing.stop().await;
    }
    async fn wait(&self) {
        self.worker.lock().await.wait().await;
        self.registry_worker.lock().await.wait().await;
        self.pairing.wait().await;
    }
}
