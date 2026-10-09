pub fn load_config<S: serde::Serialize>(
    raw_config: &str,
    env: S,
) -> Result<String, minijinja::Error> {
    let mut jinja_env = minijinja::Environment::new();
    jinja_env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    jinja_env.render_str(raw_config, env)
}

pub fn load_config_from_env<S: serde::Serialize>(
    raw_config: &str,
) -> Result<String, minijinja::Error> {
    let mut jinja_env = minijinja::Environment::new();
    jinja_env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    jinja_env.render_str(
        raw_config,
        std::env::vars().collect::<std::collections::HashMap<String, String>>(),
    )
}
