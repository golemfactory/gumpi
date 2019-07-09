#![warn(clippy::all)]
#![warn(rust_2018_idioms)]

mod async_ctrlc;
mod error;
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
        Some(_) => eprintln!("Execution interrupted..."),
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

fn gumpi_async(
    opt: Opt,
    config: JobConfig,
) -> Fallible<impl Future<Item = (), Error = failure::Error>> {
    let progname = config.progname.clone();
    let numproc = opt.numproc;
    let numthreads = opt.numthreads;
    let cpus_requested = opt.numproc; // TODO this has to be adapted when we allow threads
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
    let jobconfig_dir = opt
        .jobconfig
        .parent()
        .expect("Invalid jobconfig path")
        .to_owned();
    let noclean = opt.noclean;

    // The initialization of the provider may take time,
    // so check if the file exists at all in advance
    if let Some(input) = &config.input {
        let input_path = jobconfig_dir.join(&input.source);
        if !input_path.is_file() {
            return Err(format_err!(
                "The input data, {}, doesn't exist",
                input_path.to_string_lossy()
            ));
        }
    }

    let future = SessionMPI::init(opt.hub, prov_filter)
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
            // deploy_prefix is the location of the folder, where the executable
            // resides. See the documentation for SessionMPI::deploy for more
            // details
            let deploy_prefix = if let Some(sources) = config.sources.clone() {
                Either::A(
                    session
                        .deploy(jobconfig_dir.clone(), sources, progname)
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

            let upload_input = if let Some(input) = config.input.clone() {
                let input_path = jobconfig_dir.join(input.source);
                Either::A(session.upload_input(input_path))
            } else {
                Either::B(future::ok(()))
            };

            Either::B(
                deploy_prefix
                    .join(upload_input)
                    .and_then(move |(deploy_prefix, ())| {
                        session
                            .exec(
                                numproc,
                                numthreads,
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
                                    "Hub session references remaining, \
                                     cannot safely close the session..."
                                ))),
                            }
                        };

                        cleanup
                            .map_err(|e| error!("Error cleaning up: {}", e))
                            .then(|_| fut)
                    }),
            )
        });
    Ok(future)
}

fn run() -> Fallible<()> {
    let opt = Opt::from_args();
    let config = JobConfig::from_file(&opt.jobconfig).context("reading job config")?;

    let mut sys = System::new("gumpi");
    sys.block_on(gumpi_async(opt, config)?)
}
