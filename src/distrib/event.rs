use super::error::{BridgeError, BridgeErrorKind};
use super::message;
use super::prelude::{FromProto, GetChannelName, InstanceId, ToProto};
use crate::actor::serialized;
use prost::Message;
use redis::AsyncTypedCommands;

pub struct ResourceInner<
    K: 'static + Send + Sync + redis::ToSingleRedisArg,
    C: 'static + Send + Sync + GetChannelName,
> {
    resource_key: K,
    instance_id: InstanceId,
    namer: C,
    conn: redis::aio::ConnectionManager,
}

pub struct Resource<
    K: 'static + Send + Sync + redis::ToSingleRedisArg,
    C: 'static + Send + Sync + GetChannelName,
> {
    inner: Option<ResourceInner<K, C>>,
}

impl<
    K: 'static + Send + Sync + redis::ToSingleRedisArg,
    C: 'static + Send + Sync + GetChannelName,
> Drop for Resource<K, C>
{
    fn drop(&mut self) {
        if let Some(ResourceInner {
            resource_key,
            instance_id,
            namer,
            mut conn,
        }) = self.inner.take()
        {
            tokio::spawn(async move {
                conn.del_ex(
                    namer.get_owner_key(resource_key),
                    redis::ValueComparison::ifdeq(redis::calculate_value_digest(
                        instance_id.to_proto().encode_to_vec(),
                    )),
                )
                .await
            });
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
    to_instance_id: uuid::Uuid,
    from_instance_id: uuid::Uuid,
    inner: Req,
    _res: std::marker::PhantomData<Res>,
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

pub struct InstanceBridgeMailbox<Req, Res, C: GetChannelName> {
    pub namer: C,
    pub instance_id: InstanceId,
    pub conn: redis::aio::ConnectionManager,
    pub mbox: serialized::Mailbox<
        u64,
        message::InstanceMessage<Req, Res>,
        message::InstanceMessage<Req, Res>,
        serialized::Uint64Gen,
        redis::RedisError,
    >,
}

impl<Req, Res, C: GetChannelName> Clone for InstanceBridgeMailbox<Req, Res, C> {
    fn clone(&self) -> Self {
        Self {
            namer: self.namer.clone(),
            instance_id: self.instance_id,
            conn: self.conn.clone(),
            mbox: self.mbox.clone(),
        }
    }
}

impl<Req, Res, C: 'static + Send + Sync + GetChannelName> InstanceBridgeMailbox<Req, Res, C> {
    pub async fn own<K: 'static + Send + Sync + redis::ToSingleRedisArg + Clone>(
        &self,
        k: K,
    ) -> Result<Resource<K, C>, redis::RedisError> {
        let mut conn = self.conn.clone();
        conn.set(
            self.namer.get_owner_key(k.clone()),
            self.instance_id.to_proto().encode_to_vec(),
        )
        .await?;
        Ok(Resource {
            inner: Some(ResourceInner {
                resource_key: k,
                instance_id: self.instance_id,
                namer: self.namer.clone(),
                conn,
            }),
        })
    }
    pub async fn req<K: Send + Sync + redis::ToSingleRedisArg>(
        &self,
        k: K,
        req: Req,
        timeout: Option<tokio::time::Duration>,
    ) -> Result<Res, BridgeError> {
        let mut conn = self.conn.clone();
        let owner = match redis::AsyncCommands::get::<_, redis::Value>(
            &mut conn,
            self.namer.get_owner_key(k),
        )
        .await
        {
            Ok(owner) => uuid::Uuid::from_proto(message::Uuid::try_from(owner)?),
            Err(e) => {
                return Err(BridgeError::new(
                    BridgeErrorKind::Redis,
                    "owner lookup failed",
                    e.to_string(),
                ));
            }
        };
        let instance_req = InstanceRequest {
            to_instance_id: owner,
            from_instance_id: *self.instance_id,
            inner: req,
            _res: std::marker::PhantomData,
        };
        let res = match timeout {
            Some(timeout) => self.mbox.req_with_timeout(instance_req, timeout).await,
            None => self.mbox.req(instance_req).await,
        };
        let res = match res {
            Ok(res) => res,
            Err(serialized::Error::Timeout) => {
                return Err(BridgeError::new(
                    BridgeErrorKind::Timeout,
                    "request timed out",
                    "no reply arrived in time",
                ));
            }
            Err(serialized::Error::Drop) => {
                return Err(BridgeError::new(
                    BridgeErrorKind::Local,
                    "request dropped",
                    "the pending request was dropped before a reply arrived",
                ));
            }
            Err(serialized::Error::Closed) => {
                return Err(BridgeError::new(
                    BridgeErrorKind::Local,
                    "bridge closed",
                    "the pairing actor is no longer running",
                ));
            }
            Err(serialized::Error::Send(e)) => {
                return Err(BridgeError::new(
                    BridgeErrorKind::Redis,
                    "send failed",
                    e.to_string(),
                ));
            }
        };
        match res.inner {
            Some(message::instance_message::Inner::Res { inner }) => Ok(inner),
            Some(message::instance_message::Inner::Error { inner }) => Err(inner),
            Some(message::instance_message::Inner::Req { .. }) | None => Err(BridgeError::new(
                BridgeErrorKind::Unknown,
                "unexpected reply",
                "the reply carried no response",
            )),
        }
    }
}
