use super::ctx::RedisSubCtx;
use super::event::{InstanceBridgeMailbox, RedisSender};
use super::message;
use super::prelude::{FromProto, GetChannelName, InstanceId, OnRequest, ToProto};
use crate::actor::prelude::{ActorCtl, GetMailbox};
use crate::actor::{ActorArg, ActorStatus, ActorStatusKind, ShutdownAction, serialized};

pub struct InstanceService<Req, Res, C: GetChannelName> {
    worker: tokio::sync::Mutex<crate::async_utils::Worker<()>>,
    status: ActorStatus,
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
    pub async fn new<H: 'static + Send + Sync + OnRequest<Req, Res, E: 'static + Send + std::fmt::Display>>(
        client: redis::Client,
        instance_id: InstanceId,
        namer: C,
        handler: H,
        queue_size: usize,
        max_pending: usize,
        refresh_interval: tokio::time::Duration,
        handler_timeout: tokio::time::Duration,
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
        let (worker, status) = ActorArg {
            shutdown_action: ShutdownAction::Drain,
            ..Default::default()
        }
        .run_with_lifecycle(
            RedisSubCtx {
                instance_id,
                sender,
                mbox: mbox.clone(),
                handler,
                handler_timeout,
            },
            stream,
        );
        Ok(Self {
            worker: tokio::sync::Mutex::new(worker),
            status,
            pairing,
            mailbox: InstanceBridgeMailbox {
                namer,
                instance_id,
                conn,
                mbox,
            },
            _sink: sink,
        })
    }
}

impl<Req, Res, C: GetChannelName> InstanceService<Req, Res, C> {
    pub fn mailbox(&self) -> InstanceBridgeMailbox<Req, Res, C> {
        self.mailbox.clone()
    }
}

impl<Req, Res, C: GetChannelName> GetMailbox for InstanceService<Req, Res, C> {
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
        self.pairing.stop().await;
    }
    async fn wait(&self) {
        self.worker.lock().await.wait().await;
        self.pairing.wait().await;
    }
}
