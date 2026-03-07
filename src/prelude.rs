use crate::error::ConfigError;

pub trait ToService {
    type Service;
    fn to_service(&self) -> Result<Self::Service, ConfigError>;
}

#[async_trait::async_trait]
pub trait AsyncToService {
    type Service;
    async fn to_service(&self) -> Result<Self::Service, ConfigError>;
}

#[async_trait::async_trait]
impl<T: ToService + Send + Sync> AsyncToService for T {
    type Service = <T as ToService>::Service;
    async fn to_service(&self) -> Result<Self::Service, ConfigError> {
        ToService::to_service(self)
    }
}
