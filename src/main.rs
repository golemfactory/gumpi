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
use log::{debug, info};
use std::env;
use structopt::StructOpt;

fn show_error(e: &failure::Error) {
    match e.downcast_ref::<CtrlcEvent>() {
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

    // It's safe to call unwrap here - at this point opt.jobconfig
    // is guaranteed to be a valid filepath, which is checked by
    // JobConfig::from_file in main.rs
    let sources_dir = opt
        .jobconfig
        .parent()
        .expect("Invalid jobconfig path")
        .to_owned();

    SessionMPI::init(opt.hub, prov_filter)
        .context("initializing session")
        .and_then(move |session| {
            info!("available cores: {}", session.total_cpus());
            let cpus_available = session.total_cpus();
            if cpus_available < cpus_requested {
                return Err(format_err!(
                    "Not enough CPUs available: requested: {}, available: {}",
                    cpus_requested,
                    cpus_available
                ));
            }
            Ok(session)
        })
        .and_then(move |session| {
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

            future::ok(session)
                .join(deploy_prefix)
                .and_then(move |(session, deploy_prefix)| {
                    session
                        .exec(
                            cpus_requested,
                            config.progname,
                            config.args,
                            config.mpiargs,
                            deploy_prefix,
                        )
                        .join(future::ok(session))
                })
        })
        .and_then(|(output, session)| {
            println!("Execution output:\n{}", output);
            if let Some(outs) = output_cfg {
                Either::A(session.retrieve_output(&outs))
            } else {
                Either::B(future::ok(()))
            }
        })
        .handle_ctrlc()
        .then(|fut| {
            // TODO is the manual system stop actually needed??
            // TODO manual cleanup
            // TODO an option to disable cleanup
            // info!("Cleaning up...");
            actix::System::current().stop();
            fut
        })
}

fn run() -> Fallible<()> {
    let opt = Opt::from_args();
    let config = JobConfig::from_file(&opt.jobconfig).context("reading job config")?;

    let mut sys = System::new("gumpi");
    sys.block_on(gumpi_async(opt, config))
}
