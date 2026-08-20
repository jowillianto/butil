use super::{Actor, ActorConfig, ActorStatusKind, Context};
use crate::KeyVec;
use std::sync::Arc;

pub trait Message: 'static + Clone + Send + Sync {}

/*
 * Publisher trait
 */
pub trait Publisher<M: Message>: Send + Sync {
    fn publish(&self, m: M);
}

impl<M: Message> Publisher<M> for tokio::sync::mpsc::Sender<M> {
    fn publish(&self, m: M) {
        let _ = self.try_send(m);
    }
}

/*
 * Event for pubsub
 */
pub enum Event<M: Message> {
    Sub {
        p: Arc<dyn Publisher<M>>,
        id: uuid::Uuid,
    },
    Msg {
        msg: M,
    },
    Unsub {
        id: uuid::Uuid,
    },
}

pub struct Ctx<M: Message> {
    subs: KeyVec<uuid::Uuid, Arc<dyn Publisher<M>>>,
}

impl<M: Message> Ctx<M> {
    pub fn new() -> Self {
        Self {
            subs: KeyVec::default(),
        }
    }
}

impl<M: Message> Default for Ctx<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Message> Context<Event<M>> for Ctx<M> {
    async fn on_event(&mut self, e: Event<M>) -> bool {
        match e {
            Event::Sub { p, id } => {
                self.subs.insert_no_check(id, p);
            }
            Event::Msg { msg } => {
                for (_, p) in self.subs.iter() {
                    p.publish(msg.clone());
                }
            }
            Event::Unsub { id } => {
                self.subs.remove(&id);
            }
        }
        true
    }

    fn is_complete(&self) -> bool {
        false
    }
}

pub struct Pubsub<M: Message> {
    actor: Actor<Event<M>>,
    tx: tokio::sync::mpsc::Sender<Event<M>>,
}

impl<M: Message> Pubsub<M> {
    pub fn new(config: ActorConfig, buf_size: usize) -> Self {
        let (actor, tx) = Actor::new_bounded(config, buf_size, Ctx::new());
        Self { actor, tx }
    }
    /// Registers a publisher and hands back the id that drops it again
    pub async fn subscribe(&self, p: impl 'static + Publisher<M>) -> uuid::Uuid {
        let id = uuid::Uuid::new_v4();
        let _ = self
            .tx
            .send(Event::Sub {
                p: Arc::new(p),
                id,
            })
            .await;
        id
    }
    /// Drops the publisher that was subscribed under this id
    pub async fn unsubscribe(&self, id: uuid::Uuid) {
        let _ = self.tx.send(Event::Unsub { id }).await;
    }
    /// Fans the message out to every subscriber
    pub async fn send_mesage(&self, m: M) {
        let _ = self.tx.send(Event::Msg { msg: m }).await;
    }
    pub fn tx(&self) -> tokio::sync::mpsc::Sender<Event<M>> {
        self.tx.clone()
    }
    pub async fn stop(&self) {
        self.actor.stop().await;
    }
    pub async fn wait(&self) {
        self.actor.wait().await;
    }
    pub fn status(&self) -> ActorStatusKind {
        self.actor.status()
    }
}

pub enum TaggedEvent<M: Message> {
    Sub {
        p: Arc<dyn Publisher<M>>,
        topic: String,
        id: uuid::Uuid,
    },
    Msg {
        msg: M,
        topic: String,
    },
    Unsub {
        id: uuid::Uuid,
    },
    CloseStream {
        name: String,
    },
}

pub struct TaggedCtx<M: Message> {
    subs: KeyVec<String, KeyVec<uuid::Uuid, Arc<dyn Publisher<M>>>>,
}

impl<M: Message> TaggedCtx<M> {
    pub fn new() -> Self {
        Self {
            subs: KeyVec::default(),
        }
    }
}

impl<M: Message> Default for TaggedCtx<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Message> Context<TaggedEvent<M>> for TaggedCtx<M> {
    async fn on_event(&mut self, e: TaggedEvent<M>) -> bool {
        match e {
            TaggedEvent::Sub { p, topic, id } => match self.subs.get_mut(topic.as_str()) {
                Some(subs) => {
                    subs.insert_no_check(id, p);
                }
                None => {
                    self.subs
                        .insert_no_check(topic, KeyVec::default())
                        .insert_no_check(id, p);
                }
            },
            TaggedEvent::Msg { msg, topic } => {
                if let Some(subs) = self.subs.get(topic.as_str()) {
                    for (_, p) in subs.iter() {
                        p.publish(msg.clone());
                    }
                }
            }
            TaggedEvent::Unsub { id } => {
                for (_, subs) in self.subs.iter_mut() {
                    subs.remove(&id);
                }
            }
            TaggedEvent::CloseStream { name } => {
                self.subs.remove(name.as_str());
            }
        }
        true
    }

    fn is_complete(&self) -> bool {
        false
    }
}

pub struct MultiPubsub<M: Message> {
    actor: Actor<TaggedEvent<M>>,
    tx: tokio::sync::mpsc::Sender<TaggedEvent<M>>,
}

impl<M: Message> MultiPubsub<M> {
    pub fn new(config: ActorConfig, buf_size: usize) -> Self {
        let (actor, tx) = Actor::new_bounded(config, buf_size, TaggedCtx::new());
        Self { actor, tx }
    }
    /// Registers a publisher on one topic and hands back the id that drops it again
    pub async fn subscribe(&self, name: &str, p: impl 'static + Publisher<M>) -> uuid::Uuid {
        let id = uuid::Uuid::new_v4();
        let _ = self
            .tx
            .send(TaggedEvent::Sub {
                p: Arc::new(p),
                topic: name.to_string(),
                id,
            })
            .await;
        id
    }
    /// Drops the publisher that was subscribed under this id, whichever topic it sits on
    pub async fn unsubscribe(&self, id: uuid::Uuid) {
        let _ = self.tx.send(TaggedEvent::Unsub { id }).await;
    }
    /// Fans the message out to every subscriber of one topic
    pub async fn send_message(&self, name: &str, m: M) {
        let _ = self
            .tx
            .send(TaggedEvent::Msg {
                msg: m,
                topic: name.to_string(),
            })
            .await;
    }
    /// Drops every subscriber of one topic
    pub async fn close_stream(&self, name: &str) {
        let _ = self
            .tx
            .send(TaggedEvent::CloseStream {
                name: name.to_string(),
            })
            .await;
    }
    pub fn tx(&self) -> tokio::sync::mpsc::Sender<TaggedEvent<M>> {
        self.tx.clone()
    }
    pub async fn stop(&self) {
        self.actor.stop().await;
    }
    pub async fn wait(&self) {
        self.actor.wait().await;
    }
    pub fn status(&self) -> ActorStatusKind {
        self.actor.status()
    }
}
