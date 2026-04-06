use crate::error::ConfigError;

pub trait ToService {
    type Service;
    fn to_service(&self) -> Result<Self::Service, ConfigError>;
}

pub trait AsyncToService {
    type Service;
    fn to_service(&self) -> impl Future<Output = Result<Self::Service, ConfigError>>;
}

impl<T: ToService + Send + Sync> AsyncToService for T {
    type Service = <T as ToService>::Service;
    fn to_service(&self) -> impl Future<Output = Result<Self::Service, ConfigError>> {
        async { ToService::to_service(self) }
    }
}
