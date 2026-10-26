use super::prelude::{FromProto, ToProto};

#[derive(Clone, Copy, prost::Message)]
pub struct Uuid {
    #[prost(uint64, tag = "1")]
    first: u64,
    #[prost(uint64, tag = "2")]
    second: u64,
}

impl ToProto for uuid::Uuid {
    type Msg = Uuid;
    fn to_proto(self) -> Self::Msg {
        let (first, second) = self.as_u64_pair();
        Self::Msg { first, second }
    }
}

impl FromProto<Uuid> for uuid::Uuid {
    fn from_proto(v: Uuid) -> Self {
        Self::from_u64_pair(v.first, v.second)
    }
}

impl TryFrom<redis::Value> for Uuid {
    type Error = super::error::BridgeError;
    fn try_from(value: redis::Value) -> Result<Self, Self::Error> {
        match value {
            redis::Value::BulkString(v) => match <Self as prost::Message>::decode(v.as_slice()) {
                Ok(uuid) => Ok(uuid),
                Err(e) => Err(super::error::BridgeError::new(
                    super::error::BridgeErrorKind::Unknown,
                    "invalid uuid record",
                    e.to_string(),
                )),
            },
            redis::Value::Nil => Err(super::error::BridgeError::new(
                super::error::BridgeErrorKind::Local,
                "missing uuid record",
                "the key does not exist",
            )),
            _ => Err(super::error::BridgeError::new(
                super::error::BridgeErrorKind::Unknown,
                "invalid uuid record",
                "expected a bulk string",
            )),
        }
    }
}

pub mod instance_message {
    #[derive(prost::Oneof)]
    pub enum ProtoInner<
        Req: std::fmt::Debug + Default + prost::Message,
        Res: std::fmt::Debug + Default + prost::Message,
    > {
        #[prost(message, tag = "4")]
        Req(Req),
        #[prost(message, tag = "5")]
        Res(Res),
        #[prost(message, tag = "6")]
        Error(super::super::error::ProtoBridgeError),
    }

    #[derive(Clone)]
    pub enum Inner<Req, Res> {
        Req { inner: Req },
        Res { inner: Res },
        Error { inner: super::super::error::BridgeError },
    }
}

#[derive(prost::Message)]
pub struct ProtoInstanceMessage<
    Req: std::fmt::Debug + Default + prost::Message,
    Res: std::fmt::Debug + Default + prost::Message,
> {
    #[prost(uint64, tag = "1")]
    pub req_id: u64,
    #[prost(message, required, tag = "2")]
    pub to_instance_id: Uuid,
    #[prost(message, required, tag = "3")]
    pub from_instance_id: Uuid,
    #[prost(oneof = "instance_message::ProtoInner", tags = "4, 5, 6")]
    pub inner: Option<instance_message::ProtoInner<Req, Res>>,
}

#[derive(Clone)]
pub struct InstanceMessage<Req, Res> {
    pub req_id: u64,
    pub to_instance_id: uuid::Uuid,
    pub from_instance_id: uuid::Uuid,
    pub inner: Option<instance_message::Inner<Req, Res>>,
}

impl<Req: ToProto<Msg: std::fmt::Debug + Default>, Res: ToProto<Msg: std::fmt::Debug + Default>>
    ToProto for InstanceMessage<Req, Res>
{
    type Msg = ProtoInstanceMessage<Req::Msg, Res::Msg>;
    fn to_proto(self) -> Self::Msg {
        Self::Msg {
            req_id: self.req_id,
            to_instance_id: self.to_instance_id.to_proto(),
            from_instance_id: self.from_instance_id.to_proto(),
            inner: self.inner.map(|inner| match inner {
                instance_message::Inner::Req { inner } => {
                    instance_message::ProtoInner::Req(inner.to_proto())
                }
                instance_message::Inner::Res { inner } => {
                    instance_message::ProtoInner::Res(inner.to_proto())
                }
                instance_message::Inner::Error { inner } => {
                    instance_message::ProtoInner::Error(inner.to_proto())
                }
            }),
        }
    }
}

impl<
    P: std::fmt::Debug + Default + prost::Message,
    Q: std::fmt::Debug + Default + prost::Message,
    Req: FromProto<P>,
    Res: FromProto<Q>,
> FromProto<ProtoInstanceMessage<P, Q>> for InstanceMessage<Req, Res>
{
    fn from_proto(v: ProtoInstanceMessage<P, Q>) -> Self {
        Self {
            req_id: v.req_id,
            to_instance_id: uuid::Uuid::from_proto(v.to_instance_id),
            from_instance_id: uuid::Uuid::from_proto(v.from_instance_id),
            inner: v.inner.map(|inner| match inner {
                instance_message::ProtoInner::Req(v) => instance_message::Inner::Req {
                    inner: Req::from_proto(v),
                },
                instance_message::ProtoInner::Res(v) => instance_message::Inner::Res {
                    inner: Res::from_proto(v),
                },
                instance_message::ProtoInner::Error(v) => {
                    instance_message::Inner::Error {
                        inner: super::error::BridgeError::from_proto(v),
                    }
                }
            }),
        }
    }
}
