use failure::{Context, Fail};
use futures::Future;
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

pub trait FutureExt<F>
where
    F: Future,
{
    fn context<D>(self, context: D) -> Box<dyn Future<Item = F::Item, Error = failure::Error>>
    where
        D: Display + Send + Sync + 'static;
}

impl<F> FutureExt<F> for F
where
    F: Future + 'static,
    F::Error: Fail,
{
    fn context<D>(self, context: D) -> Box<dyn Future<Item = F::Item, Error = failure::Error>>
    where
        D: Display + Send + Sync + 'static,
    {
        Box::new(self.map_err(|e| e.context(context).into()))
    }
}
