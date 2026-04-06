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
    pub fn no_key(reason: impl Display) -> Self {
        Self {
            kind: Kind::NoKey,
            reason: format!("{}", reason),
        }
    }

    pub fn transport(reason: impl Display) -> Self {
        Self {
            kind: Kind::Transport,
            reason: format!("{}", reason),
        }
    }

    pub fn expired(reason: impl Display) -> Self {
        Self {
            kind: Kind::Expired,
            reason: format!("{}", reason),
        }
    }

    pub fn to_cache(reason: impl Display) -> Self {
        Self {
            kind: Kind::ToCache,
            reason: format!("{}", reason),
        }
    }

    pub fn from_cache(reason: impl Display) -> Self {
        Self {
            kind: Kind::FromCache,
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

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CacheError(kind: {:?}, err: {})", self.kind, self.reason)
    }
}
