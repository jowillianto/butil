#[derive(Debug)]
pub struct ConfigError {
    pub name: String,
    pub reason: String,
}

#[macro_export]
macro_rules! config_error {
  ($name:expr, $fmt:expr $(, $($args:tt)*)?) => {
    crate::error::ConfigError{
      name: $name.into(),
      reason: format!($fmt $(, $($args)*)?)
    }
  };
}
