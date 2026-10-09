pub trait HandleEvent<E: 'static + Send>: Send + Sync {
    fn handle_event(&mut self, e: E) -> impl Send + Future<Output = ()>;
}

pub trait Lifecycle<E: 'static + Send>: Send {
    /*
     * init() is guaranteed to be run at the start of a the worker
     * regardless of wether it is cancelled or not
     */
    fn init(&mut self) -> impl Send + Future<Output = ()> {
        async {}
    }
    /*
     * on_event() will run completely regardless of wether it is cancelled or not
     */
    fn on_event(&mut self, e: E) -> impl Send + Future<Output = bool> {
        async {
            let _ = e;
            true
        }
    }
    /*
     * deinit() will run at the end regardless of wether the actor is cancelled or not
     */
    fn deinit(&mut self) -> impl Send + Future<Output = ()> {
        async {}
    }
    fn is_complete(&self) -> bool {
        true
    }
}

impl<E: 'static + Send> HandleEvent<E> for tokio::sync::mpsc::Sender<E> {
    async fn handle_event(&mut self, e: E) {
        let _ = self.send(e).await;
    }
}

pub trait KvRecord<K, V> {
    type E;
    fn reg_record(
        &mut self,
        id: &K,
        value: &V,
        ttl: chrono::Duration,
    ) -> impl Send + Future<Output = Result<(), Self::E>>;
    fn del_record(
        &mut self,
        id: &K,
        ttl: chrono::Duration,
    ) -> impl Send + Future<Output = Result<(), Self::E>>;
    fn has_record(&self, id: &K) -> impl Send + Future<Output = Result<(), Self::E>>;
}

#[async_trait::async_trait]
pub trait ActorCtl: Send + Sync + std::any::Any {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    fn status(&self) -> super::actor::ActorStatusKind;
    async fn stop(&self);
    async fn wait(&self);
    async fn shutdown_and_wait(&self) {
        self.stop().await;
        self.wait().await;
    }
}

pub trait GetMailbox {
    type M: Clone;
    fn get_mailbox(&self) -> Self::M;
}
