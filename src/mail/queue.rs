use super::super::actor::{Context, Receiver, new_pair};
use super::error::Error;

pub struct Event {
    msg: lettre::Message,
    tx: tokio::sync::oneshot::Sender<Result<(), Error>>,
}

impl Event {
    pub fn new_ignore(msg: lettre::Message) -> Event {
        let (tx, _) = new_pair(tokio::time::Duration::from_secs(0));
        Event { tx, msg }
    }
    pub fn new_with_rx(
        msg: lettre::Message,
        dur: tokio::time::Duration,
    ) -> (Event, Receiver<Result<(), Error>>) {
        let (tx, rx) = new_pair(dur);
        let e = Event { tx, msg };
        (e, rx)
    }
}

pub struct Ctx<T> {
    pub transport: T,
}

impl<T> Context<Event> for Ctx<T>
where
    T: lettre::AsyncTransport<Error: std::fmt::Display> + Send + Sync,
{
    async fn on_event(&mut self, e: Event) -> bool {
        let res = self
            .transport
            .send(e.msg)
            .await
            .map(|_| ())
            .map_err(Error::new);
        let _ = e.tx.send(res);
        true
    }

    async fn deinit(&mut self) {
        self.transport.shutdown().await;
    }
}
