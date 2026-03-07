use std::fmt::Display;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Kind {
    NoKey,
    Expired,
    Transport,
    ToCache,
    FromCache,
}

#[derive(Debug, Clone)]
pub struct Error {
    kind: Kind,
    reason: String,
}

impl Error {
    pub fn no_key(reason: impl Into<String>) -> Self {
        Self::builder(Kind::NoKey).reason(reason).build()
    }

    pub fn transport(reason: impl Display) -> Self {
        Self::builder(Kind::Transport).reason_display(reason).build()
    }

    pub fn expired(reason: impl Display) -> Self {
        Self::builder(Kind::Expired).reason_display(reason).build()
    }

    pub fn to_cache(reason: impl Display) -> Self {
        Self::builder(Kind::ToCache).reason_display(reason).build()
    }

    pub fn from_cache(reason: impl Display) -> Self {
        Self::builder(Kind::FromCache).reason_display(reason).build()
    }

    pub fn builder(kind: Kind) -> ErrorBuilder {
        ErrorBuilder::new(kind)
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn reason(&self) -> &str {
        self.reason.as_str()
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CacheError(kind: {:?}, err: {})", self.kind, self.reason)
    }
}

pub struct ErrorBuilder {
    kind: Kind,
    reason: String,
}

impl ErrorBuilder {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            reason: String::new(),
        }
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = reason.into();
        self
    }

    pub fn reason_display(mut self, reason: impl Display) -> Self {
        self.reason = reason.to_string();
        self
    }

    pub fn build(self) -> Error {
        Error {
            kind: self.kind,
            reason: self.reason,
        }
    }
}
