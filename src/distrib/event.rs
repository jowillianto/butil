use super::error::{BridgeError, BridgeErrorKind};
use super::message;
use super::prelude::{GetChannelName, InstanceId, ToProto};
use crate::actor::serialized;
use prost::Message;
use redis::AsyncTypedCommands;

pub enum RegistryEvent<Req, Res> {
    Own {
        key: Vec<u8>,
        tx: tokio::sync::oneshot::Sender<Result<(), BridgeError>>,
    },
    Release {
        key: Vec<u8>,
    },
    Req {
        key: Vec<u8>,
        req: Req,
        tx: tokio::sync::oneshot::Sender<Result<Res, BridgeError>>,
    },
    Refresh,
}

pub struct ResourceOwner<Req: 'static + Send, Res: 'static + Send> {
    key: Vec<u8>,
    registry: tokio::sync::mpsc::Sender<RegistryEvent<Req, Res>>,
}

impl<Req: 'static + Send, Res: 'static + Send> Drop for ResourceOwner<Req, Res> {
    fn drop(&mut self) {
        let release = RegistryEvent::Release {
            key: std::mem::take(&mut self.key),
        };
        if let Err(tokio::sync::mpsc::error::TrySendError::Full(release)) =
            self.registry.try_send(release)
        {
            let registry = self.registry.clone();
            tokio::spawn(async move { registry.send(release).await });
        }
    }
}

#[derive(Clone)]
pub struct RedisSender<C: GetChannelName> {
    pub conn: redis::aio::ConnectionManager,
    pub namer: C,
}

impl<
    Req: Send + ToProto<Msg: std::fmt::Debug + Default>,
    Res: Send + ToProto<Msg: std::fmt::Debug + Default>,
    C: Send + Sync + GetChannelName,
> serialized::SendSerialized<message::InstanceMessage<Req, Res>> for RedisSender<C>
{
    type E = redis::RedisError;
    async fn send_serialized(&self, s: message::InstanceMessage<Req, Res>) -> Result<(), Self::E> {
        let mut conn = self.conn.clone();
        let receivers = conn
            .publish(
                self.namer.get_channel_name(InstanceId(s.to_instance_id)),
                s.to_proto().encode_to_vec(),
            )
            .await?;
        if receivers == 0 {
            return Err(redis::RedisError::from((
                redis::ErrorKind::Client,
                "no subscriber on instance channel",
            )));
        }
        Ok(())
    }
}

impl<Req, Res> serialized::GetId for message::InstanceMessage<Req, Res> {
    type Id = u64;
    fn get_id(&self) -> Self::Id {
        self.req_id
    }
}

pub struct InstanceRequest<Req, Res> {
    pub to_instance_id: uuid::Uuid,
    pub from_instance_id: uuid::Uuid,
    pub inner: Req,
    pub _res: std::marker::PhantomData<Res>,
}

impl<Req, Res> serialized::ToSerializable for InstanceRequest<Req, Res> {
    type S = message::InstanceMessage<Req, Res>;
    fn to_serializable(self, id: &<Self::S as serialized::GetId>::Id) -> Self::S {
        message::InstanceMessage {
            req_id: *id,
            to_instance_id: self.to_instance_id,
            from_instance_id: self.from_instance_id,
            inner: Some(message::instance_message::Inner::Req { inner: self.inner }),
        }
    }
}

pub struct InstanceBridgeMailbox<Req: 'static + Send, Res: 'static + Send, C: GetChannelName> {
    pub namer: C,
    pub registry: tokio::sync::mpsc::Sender<RegistryEvent<Req, Res>>,
}

impl<Req: 'static + Send, Res: 'static + Send, C: GetChannelName> Clone
    for InstanceBridgeMailbox<Req, Res, C>
{
    fn clone(&self) -> Self {
        Self {
            namer: self.namer.clone(),
            registry: self.registry.clone(),
        }
    }
}

impl<Req: 'static + Send, Res: 'static + Send, C: GetChannelName>
    InstanceBridgeMailbox<Req, Res, C>
{
    pub async fn own<K: Send + Sync + redis::ToSingleRedisArg>(
        &self,
        k: K,
    ) -> Result<ResourceOwner<Req, Res>, BridgeError> {
        let key = redis::ToRedisArgs::to_redis_args(&self.namer.get_owner_key(k)).concat();
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self
            .registry
            .send(RegistryEvent::Own {
                key: key.clone(),
                tx,
            })
            .await
            .is_err()
        {
            return Err(BridgeError::new(
                BridgeErrorKind::Local,
                "bridge closed",
                "the registry actor is no longer running",
            ));
        }
        match rx.await {
            Ok(Ok(())) => Ok(ResourceOwner {
                key,
                registry: self.registry.clone(),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BridgeError::new(
                BridgeErrorKind::Local,
                "bridge closed",
                "the registry actor dropped the claim",
            )),
        }
    }
    pub async fn req<K: Send + Sync + redis::ToSingleRedisArg>(
        &self,
        k: K,
        req: Req,
        timeout: Option<tokio::time::Duration>,
    ) -> Result<Res, BridgeError> {
        let key = redis::ToRedisArgs::to_redis_args(&self.namer.get_owner_key(k)).concat();
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self
            .registry
            .send(RegistryEvent::Req { key, req, tx })
            .await
            .is_err()
        {
            return Err(BridgeError::new(
                BridgeErrorKind::Local,
                "bridge closed",
                "the registry actor is no longer running",
            ));
        }
        let res = match timeout {
            Some(timeout) => match tokio::time::timeout(timeout, rx).await {
                Ok(res) => res,
                Err(_) => {
                    return Err(BridgeError::new(
                        BridgeErrorKind::Timeout,
                        "request timed out",
                        "no reply arrived in time",
                    ));
                }
            },
            None => rx.await,
        };
        match res {
            Ok(res) => res,
            Err(_) => Err(BridgeError::new(
                BridgeErrorKind::Local,
                "request dropped",
                "the request was dropped before a reply arrived",
            )),
        }
    }
}
