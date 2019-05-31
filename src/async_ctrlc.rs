use failure::Fail;
use futures::{future::Either, prelude::*};

#[derive(Debug, Fail)]
#[fail(display = "ctrl+c pressed")]
pub struct CtrlcEvent;

pub trait AsyncCtrlc<F>
where
    F: Future,
{
    /// Intercept ctrl+c during execution and return an error in such case
    fn handle_ctrlc(self) -> Box<dyn Future<Item = F::Item, Error = F::Error>>;
}

impl<F> AsyncCtrlc<F> for F
where
    F: Future<Error = failure::Error> + 'static,
{
    fn handle_ctrlc(self) -> Box<dyn Future<Item = F::Item, Error = F::Error>> {
        let ctrlc = tokio_signal::ctrl_c()
            .flatten_stream()
            .into_future()
            .map_err(|_| ());

        let fut = self
            .select2(ctrlc)
            .map_err(|e| match e {
                Either::A((e, _)) => e,
                _ => panic!("ctrl+c handling failed"),
            })
            .and_then(|res| match res {
                Either::A((r, _)) => Ok(r),
                Either::B(_) => Err(CtrlcEvent.into()),
            });
        Box::new(fut)
    }
}
