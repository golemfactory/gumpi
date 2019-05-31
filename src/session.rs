pub mod gu_client_ext;
pub mod mpi;

/*
fn wait_ctrlc<F>(future: F) -> Fallible<F::Item>
where
    F: Future<Error = failure::Error>,
{
    let mut sys = System::new("gumpi");
    let ctrlc = tokio_signal::ctrl_c()
        .flatten_stream()
        .into_future()
        .map_err(|_| ());
    let fut = future.select2(ctrlc);
    sys.block_on(fut)
        .map_err(|e| match e {
            Either::A((e, _)) => e,
            _ => panic!("Ctrl-C handling failed"),
        })
        .and_then(|res| match res {
            Either::A((r, _)) => Ok(r),
            Either::B(_) => {
                info!("Ctrl-C received, cleaning-up...");
                Err(format_err!("Ctrl-C received..."))
            }
        })
}
*/
