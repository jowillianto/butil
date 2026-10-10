use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use super::actor::ActorStatusKind;
use super::prelude::{ActorCtl, GetMailbox};

pub struct ActorRegistry {
    inner: HashMap<TypeId, Arc<dyn ActorCtl>>,
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
    pub fn register<T: ActorCtl>(&mut self, a: T) -> Option<T> {
        if self.inner.contains_key(&TypeId::of::<T>()) {
            return Some(a);
        }
        self.inner.insert(TypeId::of::<T>(), Arc::new(a));
        None
    }
    pub fn get_option<T: ActorCtl>(&self) -> Option<&T> {
        self.inner
            .get(&TypeId::of::<T>())
            .and_then(|a| (a.as_ref() as &dyn Any).downcast_ref::<T>())
    }
    pub fn iter(&self) -> impl Iterator<Item = &dyn ActorCtl> {
        self.inner.values().map(|a| a.as_ref())
    }
    pub fn get<T: ActorCtl>(&self) -> &T {
        self.get_option::<T>()
            .unwrap_or_else(|| panic!("actor '{}' is not registered", std::any::type_name::<T>()))
    }
    pub fn get_mailbox<T: ActorCtl + GetMailbox>(&self) -> T::M {
        self.get::<T>().get_mailbox()
    }
    pub async fn stop<T: Any>(&self) -> bool {
        let Some(a) = self.inner.get(&TypeId::of::<T>()) else {
            return false;
        };
        a.stop().await;
        true
    }
    pub async fn wait<T: Any>(&self) -> bool {
        let Some(a) = self.inner.get(&TypeId::of::<T>()) else {
            return false;
        };
        a.wait().await;
        true
    }
    pub async fn shutdown_and_wait<T: Any>(&self) -> bool {
        let Some(a) = self.inner.get(&TypeId::of::<T>()) else {
            return false;
        };
        a.shutdown_and_wait().await;
        true
    }
    pub fn list(&self) -> Vec<(&'static str, ActorStatusKind)> {
        self.inner
            .values()
            .map(|a| (a.name(), a.status()))
            .collect()
    }
    pub fn info<T: Any>(&self) -> ActorStatusKind {
        match self.inner.get(&TypeId::of::<T>()) {
            Some(a) => a.status(),
            None => ActorStatusKind::ShutdownForce,
        }
    }
}

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
