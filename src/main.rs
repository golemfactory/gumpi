#![warn(clippy::all)]
#![warn(rust_2018_idioms)]

mod async_ctrlc;
mod error;
mod failure_ext;
mod jobconfig;
mod session;

use crate::{
    async_ctrlc::{AsyncCtrlc, CtrlcEvent},
    jobconfig::{JobConfig, Opt},
    session::mpi::{DeploymentInfo, SessionMPI},
};
use actix::prelude::*;
use failure::{format_err, Fallible, ResultExt};
use failure_ext::FutureExt;
use futures::{
    future::{self, Either},
    prelude::*,
};
use log::{debug, error, info};
use std::env;
use structopt::StructOpt;

fn show_error(e: &failure::Error) {
    match e.find_root_cause().downcast_ref::<CtrlcEvent>() {
        Some(_) => eprintln!("Exection interrupted..."),
        None => {
            eprint!("Error");
            for cause in e.iter_chain() {
                eprint!(": {}", cause);
            }
            eprintln!("");
        }
    };
    std::process::exit(1);
}

fn main() {
    init_logger();
    if let Err(e) = run() {
        show_error(&e);
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info")
    }
    env_logger::init()
}

fn gumpi_async(opt: Opt, config: JobConfig) -> impl Future<Item = (), Error = failure::Error> {
    let progname = config.progname.clone();
    let cpus_requested = opt.numproc;
    let prov_filter = if opt.providers.is_empty() {
        None
    } else {
        debug!("Chosen providers: {:?}", opt.providers);
        Some(opt.providers)
    };
    let output_cfg = config.output.clone();

    // It's safe to call expect here - at this point opt.jobconfig
    // is guaranteed to be a valid filepath, which is checked by
    // JobConfig::from_file in main.rs
    let sources_dir = opt
        .jobconfig
        .parent()
        .expect("Invalid jobconfig path")
        .to_owned();
    let noclean = opt.noclean;

    SessionMPI::init(opt.hub, prov_filter)
        .handle_ctrlc()
        .context("initializing session")
        .and_then(move |session| {
            use std::rc::Rc;
            let session = Rc::new(session);
            let mut session_clone = Rc::clone(&session);

            info!("available cores: {}", session.total_cpus());
            let cpus_available = session.total_cpus();
            if cpus_available < cpus_requested {
                return Either::A(future::err(format_err!(
                    "Not enough CPUs available: requested: {}, available: {}",
                    cpus_requested,
                    cpus_available
                )));
            }
            info!("Compiling the sources...");
            let deploy_prefix = if let Some(sources) = config.sources.clone() {
                Either::A(
                    session
                        .deploy(sources_dir, sources, progname)
                        .context("deploying the sources")
                        .and_then(|depl| {
                            let DeploymentInfo {
                                logs,
                                deploy_prefix,
                            } = depl;
                            for comp in logs {
                                let logs = comp.logs.join("\n------------------\n");
                                info!(
                                    "Provider {} compilation output:\n{}",
                                    comp.node.to_string(),
                                    logs
                                );
                            }
                            Ok(Some(deploy_prefix))
                        }),
                )
            } else {
                Either::B(future::ok(None))
            };

            Either::B(
                deploy_prefix
                    .and_then(move |deploy_prefix| {
                        session
                            .exec(
                                cpus_requested,
                                config.progname,
                                config.args,
                                config.mpiargs,
                                deploy_prefix,
                            )
                            .context("program execution")
                            .join(future::ok(session))
                    })
                    .and_then(|(output, session)| {
                        println!("Execution output:\n{}", output);
                        if let Some(outs) = output_cfg {
                            Either::A(session.retrieve_output(&outs).context("retrieving output"))
                        } else {
                            Either::B(future::ok(()))
                        }
                    })
                    .handle_ctrlc()
                    .then(move |fut| {
                        // At this point, there should be no other session references
                        // remaining. If it isn't so, we want to stay on the safe side
                        // and will not attempt to cleanup.
                        info!("Cleaning up");
                        let cleanup = if noclean {
                            Either::A(future::ok(()))
                        } else {
                            match Rc::get_mut(&mut session_clone) {
                                Some(sess) => Either::B(sess.close().from_err()),
                                None => Either::A(future::err(format_err!(
                                    "Hub session refereneces remaining, \
                                     cannot safely close the session..."
                                ))),
                            }
                        };

                        cleanup
                            .map_err(|e| error!("Error cleaning up: {}", e))
                            .then(|_| fut)
                    }),
            )
        })
}

fn run() -> Fallible<()> {
    let opt = Opt::from_args();
    let config = JobConfig::from_file(&opt.jobconfig).context("reading job config")?;

    let mut sys = System::new("gumpi");
    sys.block_on(gumpi_async(opt, config))
}
