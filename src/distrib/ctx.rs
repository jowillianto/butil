use prost::Message;
use redis::AsyncTypedCommands;

use super::error::{BridgeError, BridgeErrorKind};
use super::event::{InstanceRequest, RegistryEvent};
use super::message;
use super::prelude::{FromProto, GetChannelName, InstanceId, OnRequest, ToProto};
use crate::actor::prelude::Lifecycle;
use crate::actor::serialized::SendSerialized;
use crate::actor::{self, serialized};
use crate::distrib::event::RedisSender;

pub struct RedisSubCtx<
    Req: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Req as ToProto>::Msg>,
    Res: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Res as ToProto>::Msg>,
    H: 'static + Send + Sync + OnRequest<Req, Res, E = BridgeError>,
    C: 'static + Send + Sync + GetChannelName,
> {
    pub instance_id: InstanceId,
    pub sender: RedisSender<C>,
    pub mbox: serialized::Mailbox<
        u64,
        message::InstanceMessage<Req, Res>,
        message::InstanceMessage<Req, Res>,
        serialized::Uint64Gen,
        redis::RedisError,
    >,
    pub handler: H,
    pub handler_timeout: tokio::time::Duration,
}

impl<
    Req: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Req as ToProto>::Msg>,
    Res: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Res as ToProto>::Msg>,
    H: 'static + Send + Sync + OnRequest<Req, Res, E = BridgeError>,
    C: 'static + Send + Sync + GetChannelName,
> Lifecycle<redis::Msg> for RedisSubCtx<Req, Res, H, C>
{
    async fn on_event(&mut self, e: redis::Msg) -> bool {
        let Ok(proto) =
            message::ProtoInstanceMessage::<<Req as ToProto>::Msg, <Res as ToProto>::Msg>::decode(
                e.get_payload_bytes(),
            )
        else {
            return true;
        };
        let msg = message::InstanceMessage::<Req, Res>::from_proto(proto);
        if msg.to_instance_id != *self.instance_id {
            return true;
        }
        match msg.inner {
            None => {
                /*
                 * 0. No message. Do nothing.
                 */
                ()
            }
            Some(message::instance_message::Inner::Req { inner }) => {
                /*
                 * send request via handler
                 */
                let (tx, rx) = actor::oneshot::oneshot(self.handler_timeout);
                let sender = self.sender.clone();
                let req_id = msg.req_id;
                let to_instance_id = msg.from_instance_id;
                let from_instance_id = *self.instance_id;
                /* rx will eventually finish */
                self.handler.on_request(inner, tx);
                tokio::spawn(async move {
                    let res = match rx.recv().await {
                        Ok(Ok(res)) => message::instance_message::Inner::Res { inner: res },
                        Ok(Err(e)) => message::instance_message::Inner::Error { inner: e },
                        Err(e) => message::instance_message::Inner::Error {
                            inner: match e {
                                actor::oneshot::Error::Timeout => BridgeError::new(
                                    BridgeErrorKind::Timeout,
                                    "handler timed out",
                                    "the handler did not reply within the handler timeout",
                                ),
                                actor::oneshot::Error::Drop => BridgeError::new(
                                    BridgeErrorKind::Local,
                                    "handler dropped",
                                    "the handler dropped the reply sender without replying",
                                ),
                            },
                        },
                    };
                    let _ = sender
                        .send_serialized(message::InstanceMessage::<Req, Res> {
                            req_id,
                            to_instance_id,
                            from_instance_id,
                            inner: Some(res),
                        })
                        .await;
                });
            }
            Some(message::instance_message::Inner::Res { inner }) => {
                let _ = self
                    .mbox
                    .res(message::InstanceMessage {
                        req_id: msg.req_id,
                        to_instance_id: msg.to_instance_id,
                        from_instance_id: msg.from_instance_id,
                        inner: Some(message::instance_message::Inner::Res { inner }),
                    })
                    .await;
            }
            Some(message::instance_message::Inner::Error { inner }) => {
                let _ = self
                    .mbox
                    .res(message::InstanceMessage {
                        req_id: msg.req_id,
                        to_instance_id: msg.to_instance_id,
                        from_instance_id: msg.from_instance_id,
                        inner: Some(message::instance_message::Inner::Error { inner }),
                    })
                    .await;
            }
        }
        true
    }
}

pub struct RegistryCtx<
    Req: 'static + Send,
    Res: 'static + Send,
    H: 'static + Send + Sync + OnRequest<Req, Res, E = BridgeError>,
> {
    pub instance_id: InstanceId,
    pub conn: redis::aio::ConnectionManager,
    pub ttl: tokio::time::Duration,
    pub handler: H,
    pub handler_timeout: tokio::time::Duration,
    pub mbox: serialized::Mailbox<
        u64,
        message::InstanceMessage<Req, Res>,
        message::InstanceMessage<Req, Res>,
        serialized::Uint64Gen,
        redis::RedisError,
    >,
    pub owned: std::collections::HashSet<Vec<u8>>,
}

impl<
    Req: 'static + Send,
    Res: 'static + Send,
    H: 'static + Send + Sync + OnRequest<Req, Res, E = BridgeError>,
> Lifecycle<RegistryEvent<Req, Res>> for RegistryCtx<Req, Res, H>
{
    async fn on_event(&mut self, e: RegistryEvent<Req, Res>) -> bool {
        match e {
            RegistryEvent::Own { key, tx } => {
                if self.owned.contains(&key) {
                    let _ = tx.send(Err(BridgeError::new(
                        BridgeErrorKind::Local,
                        "resource already owned",
                        "this instance already owns this resource",
                    )));
                    return true;
                }
                match self
                    .conn
                    .set_options(
                        key.clone(),
                        self.instance_id.to_proto().encode_to_vec(),
                        redis::SetOptions::default()
                            .conditional_set(redis::ExistenceCheck::NX)
                            .with_expiration(redis::SetExpiry::PX(self.ttl.as_millis() as u64)),
                    )
                    .await
                {
                    Ok(Some(_)) => {
                        self.owned.insert(key);
                        let _ = tx.send(Ok(()));
                    }
                    Ok(None) => {
                        let _ = tx.send(Err(BridgeError::new(
                            BridgeErrorKind::Local,
                            "resource already owned",
                            "another instance owns this resource",
                        )
                        .suggest("retry once the current owner releases it or its ttl expires")));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(BridgeError::new(
                            BridgeErrorKind::Redis,
                            "owner claim failed",
                            e.to_string(),
                        )));
                    }
                }
            }
            RegistryEvent::Release { key } => {
                if self.owned.remove(&key) {
                    let _ = self
                        .conn
                        .del_ex(
                            key,
                            redis::ValueComparison::ifdeq(redis::calculate_value_digest(
                                self.instance_id.to_proto().encode_to_vec(),
                            )),
                        )
                        .await;
                }
            }
            RegistryEvent::Req { key, req, tx } => {
                if self.owned.contains(&key) {
                    self.handler.on_request(req, tx);
                    return true;
                }
                let mut conn = self.conn.clone();
                let mbox = self.mbox.clone();
                let instance_id = self.instance_id;
                let handler_timeout = self.handler_timeout;
                tokio::spawn(async move {
                    let owner = match redis::AsyncCommands::get::<_, redis::Value>(&mut conn, key)
                        .await
                    {
                        Ok(owner) => match message::Uuid::try_from(owner) {
                            Ok(owner) => uuid::Uuid::from_proto(owner),
                            Err(e) => {
                                let _ = tx.send(Err(e));
                                return;
                            }
                        },
                        Err(e) => {
                            let _ = tx.send(Err(BridgeError::new(
                                BridgeErrorKind::Redis,
                                "owner lookup failed",
                                e.to_string(),
                            )));
                            return;
                        }
                    };
                    let res = match mbox
                        .req_with_timeout(
                            InstanceRequest {
                                to_instance_id: owner,
                                from_instance_id: *instance_id,
                                inner: req,
                                _res: std::marker::PhantomData,
                            },
                            handler_timeout,
                        )
                        .await
                    {
                        Ok(res) => match res.inner {
                            Some(message::instance_message::Inner::Res { inner }) => Ok(inner),
                            Some(message::instance_message::Inner::Error { inner }) => Err(inner),
                            Some(message::instance_message::Inner::Req { .. }) | None => {
                                Err(BridgeError::new(
                                    BridgeErrorKind::Unknown,
                                    "unexpected reply",
                                    "the reply carried no response",
                                ))
                            }
                        },
                        Err(serialized::Error::Timeout) => Err(BridgeError::new(
                            BridgeErrorKind::Timeout,
                            "request timed out",
                            "no reply arrived in time",
                        )),
                        Err(serialized::Error::Drop) => Err(BridgeError::new(
                            BridgeErrorKind::Local,
                            "request dropped",
                            "the pending request was dropped before a reply arrived",
                        )),
                        Err(serialized::Error::Closed) => Err(BridgeError::new(
                            BridgeErrorKind::Local,
                            "bridge closed",
                            "the pairing actor is no longer running",
                        )),
                        Err(serialized::Error::Send(e)) => Err(BridgeError::new(
                            BridgeErrorKind::Redis,
                            "send failed",
                            e.to_string(),
                        )),
                    };
                    let _ = tx.send(res);
                });
            }
            RegistryEvent::Refresh => {
                if self.owned.is_empty() {
                    return true;
                }
                let owner = self.instance_id.to_proto().encode_to_vec();
                let digest = redis::calculate_value_digest(owner.clone());
                let mut pipe = redis::pipe();
                for key in self.owned.iter() {
                    pipe.set_options(
                        key,
                        owner.clone(),
                        redis::SetOptions::default()
                            .value_comparison(redis::ValueComparison::ifdeq(digest.as_str()))
                            .with_expiration(redis::SetExpiry::PX(self.ttl.as_millis() as u64)),
                    )
                    .ignore();
                }
                let _ = pipe.exec_async(&mut self.conn).await;
            }
        }
        true
    }

    async fn deinit(&mut self) {
        let digest =
            redis::calculate_value_digest(self.instance_id.to_proto().encode_to_vec());
        for key in self.owned.drain() {
            let _ = self
                .conn
                .del_ex(key, redis::ValueComparison::ifdeq(digest.as_str()))
                .await;
        }
    }
}
