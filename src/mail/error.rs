use std::fmt::Display;

#[derive(Debug)]
pub enum Phase {
    Send,
    Shutdown,
}

#[derive(Debug)]
pub struct Error {
    phase: Phase,
    reason: String,
}

impl Error {
    pub fn send(e: impl Display) -> Self {
        Self {
            phase: Phase::Send,
            reason: format!("{}", e),
        }
    }

    pub fn shutdown(e: impl Display) -> Self {
        Self {
            phase: Phase::Shutdown,
            reason: format!("{}", e),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MailError(phase: {:?}, err: {})",
            self.phase, self.reason
        )
    }
}
