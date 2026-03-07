use super::error::Error;

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TtlValue<T>(pub T, pub chrono::NaiveDateTime);

impl<T> TtlValue<T> {
    pub fn new(value: T, expire_date: chrono::NaiveDateTime) -> Self {
        Self(value, expire_date)
    }

    pub fn value(&self) -> &T {
        &self.0
    }

    pub fn expire_date(&self) -> chrono::NaiveDateTime {
        self.1
    }

    pub fn is_expired(&self) -> bool {
        self.1 <= chrono::Utc::now().naive_utc()
    }

    pub fn error_if_expire(self) -> Result<Self, Error> {
        if self.is_expired() {
            return Err(Error::no_key(format!(
                "cache value expired at {}",
                self.expire_date()
            )));
        }
        Ok(self)
    }

    pub fn none_if_expire(self) -> Option<Self> {
        if self.is_expired() {
            return None;
        }
        Some(self)
    }

    pub fn check_to_option(value: Option<Self>) -> Option<Self> {
        value.and_then(Self::none_if_expire)
    }

    pub fn check_to_error(value: Option<Self>) -> Result<Self, Error> {
        match value {
            Some(v) => v.error_if_expire(),
            None => Err(Error::no_key("cache value is missing")),
        }
    }

    pub fn increase_ttl(&mut self, dur: chrono::Duration) {
        self.1 += dur;
    }
}

impl<T> From<(T, chrono::NaiveDateTime)> for TtlValue<T> {
    fn from((value, expire_date): (T, chrono::NaiveDateTime)) -> Self {
        Self(value, expire_date)
    }
}
