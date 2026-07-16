use std::fmt::Display;

#[derive(Debug)]
pub struct Error {
    reason: String,
}

impl Error {
    pub fn new(e: impl Display) -> Self {
        Self {
            reason: format!("{}", e),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MailError({})", self.reason)
    }
}
