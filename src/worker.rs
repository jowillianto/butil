use std::sync::atomic::AtomicBool;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct Worker<O: 'static> {
    worker: Option<tokio::task::JoinHandle<O>>,
    cancel_token: CancellationToken,
    cancel_on_drop: AtomicBool,
}

impl<O: 'static + Send + Sync> Worker<O> {
    pub fn new<U: 'static + Future<Output = O> + Send, F: FnOnce(CancellationToken) -> U>(
        f: F,
    ) -> Self {
        Self::new_on(&tokio::runtime::Handle::current(), f)
    }
    pub fn new_on<U: 'static + Future<Output = O> + Send, F: FnOnce(CancellationToken) -> U>(
        handle: &tokio::runtime::Handle,
        f: F,
    ) -> Self {
        let cancel_token = CancellationToken::new();
        let fut = f(cancel_token.clone());
        Self {
            worker: Some(handle.spawn(fut)),
            cancel_token,
            cancel_on_drop: AtomicBool::new(true),
        }
    }
    pub fn new_blocking<F: 'static + FnOnce(CancellationToken) -> O + Send + Sync>(f: F) -> Self {
        Self::new_blocking_on(&tokio::runtime::Handle::current(), f)
    }
    pub fn new_blocking_on<F: 'static + FnOnce(CancellationToken) -> O + Send + Sync>(
        handle: &tokio::runtime::Handle,
        f: F,
    ) -> Self {
        let cancel_token = CancellationToken::new();
        let cancel_token2 = cancel_token.clone();
        Self {
            worker: Some(handle.spawn_blocking(move || f(cancel_token2))),
            cancel_token,
            cancel_on_drop: AtomicBool::new(true),
        }
    }
}

impl<O: 'static> Worker<O> {
    pub fn new_local<U: 'static + Future<Output = O>, F: FnOnce(CancellationToken) -> U>(
        f: F,
    ) -> Self {
        let cancel_token = CancellationToken::new();
        let fut = f(cancel_token.clone());
        Self {
            worker: Some(tokio::task::spawn_local(fut)),
            cancel_token,
            cancel_on_drop: AtomicBool::new(true),
        }
    }
    pub async fn stop(&mut self) -> Option<O> {
        self.cancel_token.cancel();
        if let Some(worker) = self.worker.take() {
            return Some(worker.await.expect("thread panic"));
        }
        None
    }
    pub async fn wait(&mut self) -> Option<O> {
        if let Some(worker) = self.worker.take() {
            return Some(worker.await.expect("thread panic"));
        }
        None
    }
    pub fn is_running(&self) -> bool {
        !self.cancel_token.is_cancelled()
    }
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
    pub fn forget(&self) {
        self.cancel_on_drop
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

impl<O> Drop for Worker<O> {
    fn drop(&mut self) {
        if self
            .cancel_on_drop
            .load(std::sync::atomic::Ordering::Acquire)
        {
            self.cancel();
            let _ = self.worker.take();
        }
    }
}
