use std::fmt::Display;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Kind {
    NotFound,
    Io,
}

#[derive(Debug, Clone)]
pub struct Error {
    kind: Kind,
    reason: String,
}

impl Error {
    pub fn not_found(path: impl Display) -> Self {
        Self {
            kind: Kind::NotFound,
            reason: format!("{}", path),
        }
    }

    pub fn io(reason: impl Display) -> Self {
        Self {
            kind: Kind::Io,
            reason: format!("{}", reason),
        }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn reason(&self) -> &str {
        self.reason.as_str()
    }
}

impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> Self {
        Self::io(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "StorageError(kind: {:?}, err: {})",
            self.kind, self.reason
        )
    }
}
