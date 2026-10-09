use crate::async_utils::wait_for;

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

pub struct OneshotReceiver<T> {
    rx: tokio::sync::oneshot::Receiver<T>,
    timeout: tokio::time::Duration,
}

impl<T> OneshotReceiver<T> {
    pub async fn recv(self) -> Result<T, Error> {
        match wait_for(self.rx, self.timeout).await {
            Some(Ok(v)) => Ok(v),
            Some(Err(_)) => Err(Error::Drop),
            None => Err(Error::Timeout),
        }
    }
}

pub fn oneshot<T>(
    timeout: tokio::time::Duration,
) -> (tokio::sync::oneshot::Sender<T>, OneshotReceiver<T>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (tx, OneshotReceiver { rx, timeout })
}
