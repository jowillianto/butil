use std::sync::Arc;

use crate::{Worker, timer::Either};
pub enum Error {
    Timeout,
    Drop,
}
impl Error {
    pub fn is_timeout(&self) -> bool {
        matches!(self, &Self::Timeout)
    }
    pub fn is_dropped(&self) -> bool {
        matches!(self, &Self::Drop)
    }
}

pub struct RecvCtx {
    beg_noti: tokio::sync::Notify,
    end_noti: tokio::sync::Notify,
}

impl RecvCtx {
    pub fn start_countdown(&self) {
        self.beg_noti.notify_one();
    }
    pub fn end_countdown(&self) {
        self.end_noti.notify_one();
    }
    pub async fn wait_start(&self) {
        self.end_noti.notified().await;
    }
    pub async fn wait_end(&self) {
        self.end_noti.notified().await;
    }
    pub fn new(dur: tokio::time::Duration) -> (Worker<()>, Arc<RecvCtx>) {
        let ctx = Arc::new(Self {
            beg_noti: tokio::sync::Notify::new(),
            end_noti: tokio::sync::Notify::new(),
        });
        let ctx2 = ctx.clone();
        let worker = Worker::new(async move |tok| {
            ctx2.wait_start().await;
            Either::wait(tokio::time::sleep(dur), tok.cancelled()).await;
            ctx2.end_countdown();
        });
        (worker, ctx)
    }
}

pub struct Receiver<T> {
    inner: tokio::sync::oneshot::Receiver<T>,
    ctx: Arc<RecvCtx>,
    _worker: Worker<()>,
}

impl<T> Receiver<T> {
    pub async fn recv(self) -> Result<T, Error> {
        self.ctx.start_countdown();
        match Either::wait(self.ctx.wait_end(), self.inner).await {
            Either::Left(_) => Err(Error::Timeout),
            Either::Right(Ok(v)) => Ok(v),
            Either::Right(Err(_)) => Err(Error::Drop),
        }
    }
}

pub fn new_pair<T>(
    timeout: tokio::time::Duration,
) -> (tokio::sync::oneshot::Sender<T>, Receiver<T>) {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let (worker, ctx) = RecvCtx::new(timeout);
    let receiver = Receiver {
        inner: receiver,
        ctx,
        _worker: worker,
    };
    (sender, receiver)
}
