use std::sync::atomic::AtomicBool;
use tokio_util::sync::CancellationToken;

pub struct WorkerArg<'a, F> {
    func: F,
    cancel_token: Option<CancellationToken>,
    handle: Option<&'a tokio::runtime::Handle>,
}

impl<F> WorkerArg<'static, F> {
    pub fn new<U>(func: F) -> Self
    where
        F: FnOnce(CancellationToken) -> U,
    {
        Self {
            func,
            cancel_token: None,
            handle: None,
        }
    }
}
impl<'a, F> WorkerArg<'a, F> {
    pub fn with_cancel_token(mut self, c: CancellationToken) -> Self {
        self.cancel_token = Some(c);
        self
    }
    pub fn with_handle<'b>(self, h: &'b tokio::runtime::Handle) -> WorkerArg<'b, F> {
        WorkerArg {
            func: self.func,
            cancel_token: self.cancel_token,
            handle: Some(h),
        }
    }
}

impl<
    'a,
    O: 'static + Send + Sync,
    U: 'static + Future<Output = O> + Send,
    F: FnOnce(CancellationToken) -> U,
> WorkerArg<'a, F>
{
    pub fn spawn(mut self) -> Worker<O> {
        let cancel_token = self.cancel_token.take().unwrap_or_default();
        let fut = (self.func)(cancel_token.clone());
        let token = cancel_token.clone();
        let fut = async move {
            let out = fut.await;
            token.cancel();
            out
        };
        let worker = match self.handle {
            Some(h) => h.spawn(fut),
            None => tokio::runtime::Handle::current().spawn(fut),
        };
        Worker {
            worker: Some(worker),
            cancel_token,
            cancel_on_drop: AtomicBool::new(true),
        }
    }
}

impl<'a, O: 'static, U: 'static + Future<Output = O>, F: FnOnce(CancellationToken) -> U>
    WorkerArg<'a, F>
{
    /// The handle is ignored; the task is spawned on the current `LocalSet`.
    pub fn spawn_local(mut self) -> Worker<O> {
        let cancel_token = self.cancel_token.take().unwrap_or_default();
        let fut = (self.func)(cancel_token.clone());
        let token = cancel_token.clone();
        let fut = async move {
            let out = fut.await;
            token.cancel();
            out
        };
        Worker {
            worker: Some(tokio::task::spawn_local(fut)),
            cancel_token,
            cancel_on_drop: AtomicBool::new(true),
        }
    }
}

#[derive(Debug)]
pub struct Worker<O: 'static> {
    worker: Option<tokio::task::JoinHandle<O>>,
    cancel_token: CancellationToken,
    cancel_on_drop: AtomicBool,
}

impl<O: 'static> Worker<O> {
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
        self.worker.is_some()
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
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
