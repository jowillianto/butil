use super::Worker;

#[async_trait::async_trait]
pub trait ListenerMut<E: Send + Sync> {
    async fn invoke(&mut self, e: E);
}

#[async_trait::async_trait]
pub trait Listener<E: Send + Sync> {
    async fn invoke(&self, e: E);
}

pub trait Dispatch<E: Send + Sync> {
    fn emit(&self, e: E) -> impl Future<Output = bool> + Send + Sync;
}

impl<E: Send + Sync> Dispatch<E> for tokio::sync::mpsc::Sender<E> {
    async fn emit(&self, e: E) -> bool {
        self.send(e).await.is_ok()
    }
}

impl<E: Send + Sync> Dispatch<E> for tokio::sync::mpsc::UnboundedSender<E> {
    async fn emit(&self, e: E) -> bool {
        self.send(e).is_ok()
    }
}

pub fn spawn_actor<E: 'static + Send + Sync>(
    listener: impl 'static + Listener<E> + Send,
    size: usize,
) -> (Worker<()>, tokio::sync::mpsc::Sender<E>) {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(size);
    let worker = Worker::new(async move |cancel_token| {
        while let Some(e) = receiver.recv().await {
            listener.invoke(e).await;
        }
        cancel_token.cancel();
    });
    (worker, sender)
}
