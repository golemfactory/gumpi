use failure::Context;
use std::fmt::Display;

pub trait OptionExt<T> {
    fn ok_or_context<D>(self, context: D) -> Result<T, Context<D>>
    where
        D: Display + Send + Sync + 'static;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_context<D>(self, context: D) -> Result<T, Context<D>>
    where
        D: Display + Send + Sync + 'static,
    {
        self.ok_or_else(|| Context::new(context))
    }
}
