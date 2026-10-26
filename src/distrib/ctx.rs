use prost::Message;

use super::error::{BridgeError, BridgeErrorKind};
use super::message;
use super::prelude::{FromProto, GetChannelName, InstanceId, OnRequest, ToProto};
use crate::actor::prelude::Lifecycle;
use crate::actor::serialized::SendSerialized;
use crate::actor::{self, serialized};
use crate::distrib::event::RedisSender;

pub struct RedisSubCtx<
    Req: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Req as ToProto>::Msg>,
    Res: 'static + Send + ToProto<Msg: std::fmt::Debug + Default> + FromProto<<Res as ToProto>::Msg>,
    H: 'static + Send + Sync + OnRequest<Req, Res, E: 'static + Send + std::fmt::Display>,
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
    H: 'static + Send + Sync + OnRequest<Req, Res, E: 'static + Send + std::fmt::Display>,
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
                        Ok(Err(e)) => message::instance_message::Inner::Error {
                            inner: BridgeError::new(
                                BridgeErrorKind::Local,
                                "handler error",
                                e.to_string(),
                            ),
                        },
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
