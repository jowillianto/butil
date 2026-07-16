use std::collections::HashMap;
use std::fmt::Display;

#[derive(Debug)]
pub struct Error {
    reason: String,
}

impl Error {
    pub fn parse(e: impl Display) -> Self {
        Self {
            reason: format!("{}", e),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MailTemplateError(err: {})", self.reason)
    }
}

pub trait ParseMailTemplate<Ctx> {
    fn parse_mail_template(&self, ctx: &Ctx) -> Result<String, Error>;
}

pub struct JjinjaTemplate {
    template: String,
}

impl ParseMailTemplate<HashMap<String, String>> for JjinjaTemplate {
    fn parse_mail_template(&self, ctx: &HashMap<String, String>) -> Result<String, Error> {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.render_str(&self.template, ctx).map_err(Error::parse)
    }
}
