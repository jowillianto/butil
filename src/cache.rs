use std::any::Any;
use std::sync::Arc;

use crate::actor::{Actor, ActorConfig, Context, ShutdownAction};
use crate::{BoundedKeyVec, Worker, wait_or};
#[cfg(feature = "cache-redis")]
use redis::AsyncCommands;

type Val = Arc<dyn Any + Send + Sync>;

struct Entry {
    value: Val,
    expires_at: chrono::DateTime<chrono::Utc>,
}

pub enum Event {
    Get {
        key: String,
        tx: tokio::sync::oneshot::Sender<Option<Val>>,
    },
    Set {
        key: String,
        value: Val,
        ttl: chrono::Duration,
    },
    Del {
        key: String,
    },
    Refresh,
}

struct CacheCtx {
    map: BoundedKeyVec<String, Entry>,
}

impl Context<Event> for CacheCtx {
    async fn on_event(&mut self, e: Event) -> bool {
        let now = chrono::Utc::now();
        match e {
            Event::Get { key, tx } => {
                let hit = match self.map.get(key.as_str()) {
                    Some(entry) if entry.expires_at > now => Some(entry.value.clone()),
                    _ => None,
                };
                let _ = tx.send(hit);
            }
            Event::Set { key, value, ttl } => {
                self.map.remove(key.as_str());
                self.map.insert_no_check(
                    key,
                    Entry {
                        value,
                        expires_at: now + ttl,
                    },
                );
            }
            Event::Del { key } => {
                self.map.remove(key.as_str());
            }
            Event::Refresh => {
                self.map.filter_self(|(_, entry)| entry.expires_at > now);
            }
        }
        true
    }

    fn is_complete(&self) -> bool {
        false
    }
}

pub struct InMemoryCache {
    #[allow(dead_code)]
    actor: Actor<Event>,
    tx: tokio::sync::mpsc::Sender<Event>,
    #[allow(dead_code)]
    refresh_worker: Worker<()>,
}

impl InMemoryCache {
    pub fn new(
        max_size: usize,
        buf_size: usize,
        refresh_interval: tokio::time::Duration,
    ) -> Self {
        let (actor, tx) = Actor::new_bounded(
            ActorConfig {
                shutdown_action: ShutdownAction::Drain,
            },
            buf_size,
            CacheCtx {
                map: BoundedKeyVec::new(max_size),
            },
        );
        let refresh_tx = tx.clone();
        let refresh_worker = Worker::new(async move |cancel_token| {
            let mut interval = tokio::time::interval(refresh_interval);
            while wait_or(interval.tick(), cancel_token.cancelled())
                .await
                .is_some()
            {
                if refresh_tx.send(Event::Refresh).await.is_err() {
                    break;
                }
            }
        });
        Self {
            actor,
            tx,
            refresh_worker,
        }
    }

    pub async fn set<V: 'static + Send + Sync>(&self, k: &str, v: V, ttl: chrono::Duration) {
        let _ = self
            .tx
            .send(Event::Set {
                key: k.to_string(),
                value: Arc::new(v),
                ttl,
            })
            .await;
    }

    pub async fn get<V: 'static + Send + Sync>(&self, k: &str) -> Option<Arc<V>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Event::Get {
                key: k.to_string(),
                tx,
            })
            .await
            .ok()?;
        rx.await.ok()?.and_then(|v| v.downcast::<V>().ok())
    }

    pub async fn del(&self, k: &str) {
        let _ = self.tx.send(Event::Del { key: k.to_string() }).await;
    }
}

#[cfg(feature = "cache-redis")]
#[derive(Debug)]
pub enum Error {
    Redis(redis::RedisError),
    Json(serde_json::Error),
}

#[cfg(feature = "cache-redis")]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Redis(e) => write!(f, "redis: {}", e),
            Self::Json(e) => write!(f, "json: {}", e),
        }
    }
}

#[cfg(feature = "cache-redis")]
impl std::error::Error for Error {}

#[cfg(feature = "cache-redis")]
impl From<redis::RedisError> for Error {
    fn from(e: redis::RedisError) -> Self {
        Self::Redis(e)
    }
}

#[cfg(feature = "cache-redis")]
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[cfg(feature = "cache-redis")]
pub struct RedisCache {
    conn: redis::aio::MultiplexedConnection,
}

#[cfg(feature = "cache-redis")]
impl RedisCache {
    pub async fn connect(
        conn_info: impl redis::IntoConnectionInfo,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(conn_info)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }

    pub async fn set<V: serde::Serialize>(
        &self,
        k: &str,
        v: &V,
        ttl: chrono::Duration,
    ) -> Result<(), Error> {
        let payload = serde_json::to_string(v)?;
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(k, payload, ttl.num_seconds().max(0) as u64)
            .await?;
        Ok(())
    }

    pub async fn get<V: serde::de::DeserializeOwned>(
        &self,
        k: &str,
        ttl: chrono::Duration,
    ) -> Result<Option<V>, Error> {
        let mut conn = self.conn.clone();
        let raw: Option<String> = conn
            .get_ex(k, redis::Expiry::EX(ttl.num_seconds().max(0) as u64))
            .await?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub async fn del(&self, k: &str) -> Result<bool, Error> {
        let mut conn = self.conn.clone();
        let n: i64 = conn.del(k).await?;
        Ok(n > 0)
    }
}
