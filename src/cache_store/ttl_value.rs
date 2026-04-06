use super::prelude::IsExpired;
use crate::cache_store::prelude::IntoValue;

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

    pub fn increase_ttl(&mut self, dur: chrono::Duration) {
        self.1 += dur;
    }
}

impl<T> From<(T, chrono::NaiveDateTime)> for TtlValue<T> {
    fn from((value, expire_date): (T, chrono::NaiveDateTime)) -> Self {
        Self(value, expire_date)
    }
}

impl<T> IsExpired for TtlValue<T> {
    fn is_expired(&self) -> bool {
        self.1 <= chrono::Utc::now().naive_utc()
    }
}

impl<T> IntoValue for TtlValue<T> {
    type Target = T;
    fn into_value(self) -> T {
        self.0
    }
}
