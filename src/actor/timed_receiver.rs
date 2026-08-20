use crate::wait_for;

/// Receiver errors.
/// Timeout: could be timeout
/// Drop: the sender is dropped
pub enum TimedReceiverError {
    Timeout,
    Drop,
}
impl TimedReceiverError {
    pub fn is_timeout(&self) -> bool {
        matches!(self, &Self::Timeout)
    }
    pub fn is_dropped(&self) -> bool {
        matches!(self, &Self::Drop)
    }
}

/// Creates a receiver which recv function has a timeout
pub struct TimedReceiver<T> {
    rx: tokio::sync::oneshot::Receiver<T>,
    dur: tokio::time::Duration,
}

impl<T> TimedReceiver<T> {
    pub async fn recv(self) -> Result<T, TimedReceiverError> {
        match wait_for(self.rx, self.dur).await {
            Some(Ok(v)) => Ok(v),
            Some(Err(_)) => Err(TimedReceiverError::Drop),
            None => Err(TimedReceiverError::Timeout),
        }
    }
    pub fn new(dur: tokio::time::Duration) -> (tokio::sync::oneshot::Sender<T>, Self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (tx, Self { rx, dur })
    }
}
