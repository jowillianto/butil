use std::collections::HashMap;

/* Jinja `{{ VAR }}` substitution for config files. Read the file as a string,
 * `ConfigLoader::new_from_env().replace(&raw)` to expand placeholders, then
 * hand the substituted string to your config parser (toml, yaml, etc.).
 *
 * Unknown placeholders make `replace` error, naming the missing variable. */
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

    pub fn replace(&self, input: &str) -> Result<String, minijinja::Error> {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.render_str(input, &self.vars)
    }
}
