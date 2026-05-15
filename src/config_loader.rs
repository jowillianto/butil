use regex::{Captures, Regex};
use std::collections::HashMap;
use std::sync::LazyLock;

static VAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([A-Za-z0-9_]+)\}").expect("config_loader::var_regex"));

/* Simple `${VAR}` substitution for config files. Read the file as a string,
 * `ConfigLoader::new_from_env().replace(&raw)` to expand placeholders, then
 * hand the substituted string to your config parser (toml, yaml, etc.).
 *
 * Unknown placeholders are left in place so a downstream parse error makes
 * the missing variable visible. `$VAR` (no braces) is not recognised. */
pub struct ConfigLoader {
    vars: HashMap<String, String>,
}

impl ConfigLoader {
    pub fn new(vars: HashMap<String, String>) -> Self {
        Self { vars }
    }

    pub fn new_from_env() -> Self {
        Self {
            vars: std::env::vars().collect(),
        }
    }

    pub fn replace(&self, input: &str) -> String {
        VAR_RE
            .replace_all(input, |caps: &Captures| match self.vars.get(&caps[1]) {
                Some(v) => v.clone(),
                None => caps[0].to_string(),
            })
            .into_owned()
    }
}
