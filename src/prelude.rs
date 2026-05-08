use crate::error::ConfigError;
use atomic_enum::atomic_enum;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/* Lifecycle phase shared across long-lived services. Wrapped by the
 * `atomic_enum` macro so a service can publish its current phase to
 * external observers without taking a lock. */
#[atomic_enum]
#[derive(PartialEq, Eq)]
pub enum ServicePhase {
    Init,
    Active,
    Stopping,
    Shutdown,
}

/* Cloneable handle to a shared `ServicePhase`. Transitions are CAS-only
 * and idempotent: an invalid transition is a no-op rather than an error.
 *
 * Allowed edges:
 *   Init     -> Active            (activate)
 *   Active   -> Stopping          (stop)
 *   Active   -> Shutdown          (shutdown)
 *   Stopping -> Shutdown          (shutdown)
 */
#[derive(Debug, Clone)]
pub struct Status {
    phase: Arc<AtomicServicePhase>,
}

impl Status {
    pub fn new() -> Self {
        Self {
            phase: Arc::new(AtomicServicePhase::new(ServicePhase::Init)),
        }
    }

    pub fn phase(&self) -> ServicePhase {
        self.phase.load(Ordering::Acquire)
    }

    pub fn activate(&self) {
        let _ = self.phase.compare_exchange(
            ServicePhase::Init,
            ServicePhase::Active,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub fn stop(&self) {
        let _ = self.phase.compare_exchange(
            ServicePhase::Active,
            ServicePhase::Stopping,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub fn shutdown(&self) {
        let current = ServicePhase::Active;
        while self
            .phase
            .compare_exchange_weak(
                current,
                ServicePhase::Shutdown,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            if current == ServicePhase::Init || current == ServicePhase::Shutdown {
                break;
            }
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::new()
    }
}

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
