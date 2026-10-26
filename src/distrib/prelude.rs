pub trait ToProto {
    type Msg: prost::Message;
    fn to_proto(self) -> Self::Msg;
}

pub trait FromProto<Msg: prost::Message> {
    fn from_proto(v: Msg) -> Self;
}

pub trait OnRequest<Req, Res> {
    type E;
    fn on_request(&self, request: Req, tx: tokio::sync::oneshot::Sender<Result<Res, Self::E>>);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InstanceId(pub uuid::Uuid);

impl Default for InstanceId {
    fn default() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl std::ops::Deref for InstanceId {
    type Target = uuid::Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait GetChannelName: Clone {
    fn get_channel_name(
        &self,
        instance_id: InstanceId,
    ) -> impl Send + Sync + redis::ToSingleRedisArg;
    fn get_owner_key<K: Send + Sync + redis::ToSingleRedisArg>(
        &self,
        k: K,
    ) -> impl Send + Sync + redis::ToSingleRedisArg;
}
