use super::error::Error;

#[async_trait::async_trait]
pub trait Service {
    async fn send(&self, msg: lettre::Message) -> Result<(), Error>;
    async fn shutdown(&self) -> Result<(), Error>;
}

#[async_trait::async_trait]
impl<E: lettre::AsyncTransport<Error: std::fmt::Display> + Send + Sync> Service for E {
    async fn send(&self, msg: lettre::Message) -> Result<(), Error> {
        <E as lettre::AsyncTransport>::send(self, msg)
            .await
            .map_err(Error::send)
            .map(|_| ())
    }

    async fn shutdown(&self) -> Result<(), Error> {
        <E as lettre::AsyncTransport>::shutdown(self).await;
        Ok(())
    }
}
