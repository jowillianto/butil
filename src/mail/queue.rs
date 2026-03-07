use std::future::Future;

use super::error::Error;
use super::prelude::Service;

pub struct Queue {
    sender: tokio::sync::mpsc::Sender<lettre::Message>,
    worker: Option<tokio::task::JoinHandle<()>>,
    cancel_token: tokio_util::sync::CancellationToken,
}

impl Queue {
    pub fn new<O: Send + Future<Output = ()>, F: 'static + Send + Fn(Error) -> O>(
        s: impl 'static + Service + Send,
        queue_size: usize,
        on_err: F,
    ) -> Self {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(queue_size);
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let worker_token = cancel_token.clone();
        let worker = tokio::task::spawn(async move {
            while let Some(msg) = tokio::select! {
                msg = receiver.recv() => msg,
                _ = worker_token.cancelled() => None
            } {
                if let Err(e) = s.send(msg).await {
                    on_err(e).await;
                }
            }
            if let Err(e) = s.shutdown().await {
                on_err(e).await;
            }
        });
        Self {
            sender,
            worker: Some(worker),
            cancel_token,
        }
    }

    pub async fn stop(&mut self) {
        self.cancel_token.cancel();
        if let Some(worker) = self.worker.take() {
            worker.await.expect("no panic");
        }
    }

    pub async fn send(&self, mail: lettre::Message) {
        self.sender.send(mail).await.expect("cannot err")
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        self.worker.take();
    }
}
