use super::record::Record;
use super::wire::{
    AsyncOnReq, AsyncWireOut, GenId, GetId, ToRawRequest, WireActor, WireError, WireEvent,
};
use super::{Actor, ActorConfig, Context, ShutdownAction};
use redis::AsyncTypedCommands;

/*
 * Redis messages
 */
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RedisMsg<Req, Res> {
    Req {
        inner: Req,
        producer_instance_id: uuid::Uuid,
    },
    Res {
        inner: Res,
        producer_instance_id: uuid::Uuid,
    },
}

/*
 * Redis request wire
 */
#[derive(Clone)]
pub struct RedisReqWire {
    conn: redis::aio::MultiplexedConnection,
    channel: String,
    instance_id: uuid::Uuid,
}

impl<Req: serde::Serialize + Send> AsyncWireOut<Req> for RedisReqWire {
    type Error = redis::RedisError;
    async fn wire_out(&mut self, msg: Req) -> Result<(), Self::Error> {
        let payload = serde_json::to_string(&RedisMsg::<Req, ()>::Req {
            inner: msg,
            producer_instance_id: self.instance_id,
        })
        .expect("json parse error");
        self.conn.publish(&self.channel, &payload).await?;
        Ok(())
    }
}

/* Redis response wire */
#[derive(Clone)]
pub struct RedisResWire {
    conn: redis::aio::MultiplexedConnection,
    channel: String,
    instance_id: uuid::Uuid,
}
impl<Res: serde::Serialize + Send> AsyncWireOut<Res> for RedisResWire {
    type Error = redis::RedisError;
    async fn wire_out(&mut self, msg: Res) -> Result<(), Self::Error> {
        let payload = serde_json::to_string(&RedisMsg::<(), Res>::Res {
            inner: msg,
            producer_instance_id: self.instance_id,
        })
        .expect("json parse error");
        self.conn.publish(&self.channel, &payload).await?;
        Ok(())
    }
}

/*
 * Redis subscriber. Requests run through the handler and any response is
 * published via `RedisResWire`; responses are forwarded into `req_res_tx`
 * to green-light the matching pending request in the `WireActor`.
 */
pub struct RedisSub<Id, RawReq, RawRes, Req, InW, H>
where
    InW: AsyncWireOut<RawReq>,
    H: AsyncOnReq<RawReq, RawRes>,
{
    wire: RedisResWire,
    handler: H,
    instance_id: uuid::Uuid,
    req_res_tx: tokio::sync::mpsc::Sender<WireEvent<RawReq, InW, Req, RawRes, Id>>,
}

impl<Id, RawReq, RawRes, Req, InW, H> RedisSub<Id, RawReq, RawRes, Req, InW, H>
where
    Id: Send + 'static,
    RawReq: serde::de::DeserializeOwned + Send + 'static,
    RawRes: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    Req: Send + 'static,
    InW: AsyncWireOut<RawReq> + 'static,
    InW::Error: Send + 'static,
    H: AsyncOnReq<RawReq, RawRes> + Send + 'static,
{
    pub async fn new(
        client: &redis::Client,
        channel: &str,
        config: ActorConfig,
        instance_id: uuid::Uuid,
        req_res_tx: tokio::sync::mpsc::Sender<WireEvent<RawReq, InW, Req, RawRes, Id>>,
        handler: H,
    ) -> Result<Actor<redis::Msg>, redis::RedisError> {
        let mut pubsub = client.get_async_pubsub().await?;
        pubsub.subscribe(channel).await?;
        Ok(Actor::new(
            config,
            Self {
                wire: RedisResWire {
                    conn: client.get_multiplexed_async_connection().await?,
                    channel: channel.into(),
                    instance_id,
                },
                handler,
                instance_id,
                req_res_tx,
            },
            pubsub.into_on_message(),
        ))
    }
}

impl<Id, RawReq, RawRes, Req, InW, H> Context<redis::Msg>
    for RedisSub<Id, RawReq, RawRes, Req, InW, H>
where
    Id: Send + 'static,
    RawReq: serde::de::DeserializeOwned + Send + 'static,
    RawRes: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    Req: Send + 'static,
    InW: AsyncWireOut<RawReq> + 'static,
    InW::Error: Send + 'static,
    H: AsyncOnReq<RawReq, RawRes> + Send + 'static,
{
    async fn on_event(&mut self, msg: redis::Msg) -> bool {
        let Ok(payload) = msg.get_payload::<String>() else {
            return true;
        };
        match serde_json::from_str::<RedisMsg<RawReq, RawRes>>(&payload) {
            Ok(RedisMsg::Req {
                inner,
                producer_instance_id,
            }) => {
                if producer_instance_id != self.instance_id
                    && let Some(res) = self.handler.on_req(inner).await
                {
                    let _ = self.wire.wire_out(res).await;
                }
            }
            Ok(RedisMsg::Res {
                inner,
                producer_instance_id,
            }) => {
                if producer_instance_id != self.instance_id {
                    let _ = self.req_res_tx.send(WireEvent::Res { res: inner }).await;
                }
            }
            Err(_) => {}
        }
        true
    }
}
pub struct RedisBridge<Id, G, RawReq, RawRes, Req>
where
    Id: Eq + Clone + Send + Sync + 'static,
    G: GenId<Id> + Send + Sync + 'static,
    RawReq: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    RawRes: GetId<Id> + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    Req: Send + Sync + 'static,
{
    req_res: WireActor<Id, G, RawReq, RawRes, Req, RedisReqWire>,
    redis_sub: Actor<redis::Msg>,
}

impl<Id, G, RawReq, RawRes, Req> RedisBridge<Id, G, RawReq, RawRes, Req>
where
    Id: Eq + Clone + Send + Sync + 'static,
    G: GenId<Id> + Send + Sync + 'static,
    RawReq: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    RawRes: GetId<Id> + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    Req: Send + Sync + 'static,
{
    pub async fn new<H: AsyncOnReq<RawReq, RawRes> + Send + 'static>(
        client: &redis::Client,
        channel: String,
        config: ActorConfig,
        buf_size: usize,
        max_pending: usize,
        timeout: tokio::time::Duration,
        cleanup_threshold: tokio::time::Duration,
        instance_id: uuid::Uuid,
        id_gen: G,
        handler: H,
    ) -> Result<Self, redis::RedisError>
    where
        Req: ToRawRequest<RawReq, Id>,
    {
        let req_res = WireActor::new(
            config,
            buf_size,
            max_pending,
            timeout,
            cleanup_threshold,
            id_gen,
            RedisReqWire {
                conn: client.get_multiplexed_async_connection().await?,
                channel: channel.clone(),
                instance_id,
            },
        );
        let redis_sub = RedisSub::new(
            client,
            &channel,
            ActorConfig {
                shutdown_action: ShutdownAction::Force,
            },
            instance_id,
            req_res.tx(),
            handler,
        )
        .await?;
        Ok(Self { req_res, redis_sub })
    }

    pub async fn req(&self, req: Req) -> Result<RawRes, WireError<redis::RedisError>> {
        self.req_res.req(req).await
    }

    pub async fn fire(&self, req: Req) -> Result<(), WireError<redis::RedisError>> {
        self.req_res.fire(req).await
    }

    pub fn mailbox(&self) -> super::wire::WireActorMailbox<Id, RawReq, RawRes, Req, RedisReqWire> {
        self.req_res.mailbox()
    }

    pub async fn shutdown(&self) {
        self.redis_sub.stop().await;
        self.req_res.stop().await;
    }

    pub async fn wait(&self) {
        self.redis_sub.wait().await;
        self.req_res.wait().await;
    }
}

/*
 * Redis record
 */
pub struct RedisRecord {
    conn: redis::aio::MultiplexedConnection,
    prefix: String,
}

impl RedisRecord {
    pub async fn new(
        client: &redis::Client,
        prefix: impl Into<String>,
    ) -> Result<Self, redis::RedisError> {
        Ok(Self {
            conn: client.get_multiplexed_async_connection().await?,
            prefix: prefix.into(),
        })
    }
}

impl<Id: std::fmt::Display + Sync> Record<Id> for RedisRecord {
    type Error = redis::RedisError;

    async fn reg(&mut self, id: &Id, ttl: tokio::time::Duration) -> Result<(), redis::RedisError> {
        self.conn
            .pset_ex(
                format!("{}{}", self.prefix, id),
                "1",
                ttl.as_millis() as u64,
            )
            .await
    }

    async fn unreg(&mut self, id: &Id) -> Result<(), redis::RedisError> {
        self.conn.del(format!("{}{}", self.prefix, id)).await?;
        Ok(())
    }

    async fn has(&mut self, id: &Id) -> Result<bool, redis::RedisError> {
        self.conn.exists(format!("{}{}", self.prefix, id)).await
    }
}
