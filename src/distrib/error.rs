use super::prelude::{FromProto, ToProto};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum BridgeErrorKind {
    Unknown = 0,
    Local = 1,
    Redis = 2,
    Timeout = 3,
}

impl std::fmt::Display for BridgeErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "UNKNOWN"),
            Self::Local => write!(f, "LOCAL"),
            Self::Redis => write!(f, "REDIS"),
            Self::Timeout => write!(f, "TIMEOUT"),
        }
    }
}

#[derive(prost::Message)]
pub struct ProtoBridgeError {
    #[prost(enumeration = "BridgeErrorKind", tag = "1")]
    kind: i32,
    #[prost(string, tag = "2")]
    title: String,
    #[prost(string, tag = "3")]
    desc: String,
    #[prost(string, repeated, tag = "4")]
    suggestions: Vec<String>,
}

impl ToProto for BridgeError {
    type Msg = ProtoBridgeError;
    fn to_proto(self) -> Self::Msg {
        Self::Msg {
            kind: self.kind as i32,
            title: self.title,
            desc: self.desc,
            suggestions: self.suggestions,
        }
    }
}

impl FromProto<ProtoBridgeError> for BridgeError {
    fn from_proto(v: ProtoBridgeError) -> Self {
        Self {
            kind: BridgeErrorKind::try_from(v.kind).unwrap_or_default(),
            title: v.title,
            desc: v.desc,
            suggestions: v.suggestions,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BridgeError {
    kind: BridgeErrorKind,
    title: String,
    desc: String,
    suggestions: Vec<String>,
}

impl BridgeError {
    pub fn new(kind: BridgeErrorKind, title: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            desc: desc.into(),
            suggestions: Vec::new(),
        }
    }
    pub fn suggest(mut self, suggestion: impl Into<String>) -> Self {
        self.suggest_mut(suggestion);
        self
    }
    pub fn suggest_mut(&mut self, suggestion: impl Into<String>) {
        self.suggestions.push(suggestion.into());
    }
    pub fn kind(&self) -> BridgeErrorKind {
        self.kind
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn desc(&self) -> &str {
        &self.desc
    }
    pub fn suggestions(&self) -> &[String] {
        &self.suggestions
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BridgeError::{}: {}: {}", self.kind, self.title, self.desc)?;
        for suggestion in &self.suggestions {
            write!(f, "\n- suggest: {}", suggestion)?;
        }
        Ok(())
    }
}

impl std::error::Error for BridgeError {}
