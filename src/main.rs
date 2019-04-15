#![warn(clippy::all)]
#![warn(rust_2018_idioms)]
mod failure_ext;
mod jobconfig;
mod session;

use crate::{
    jobconfig::{JobConfig, Opt},
    session::mpi::{DeploymentInfo, SessionMPI},
};
use actix::prelude::*;
use failure::{format_err, Fallible, ResultExt};
use failure_ext::FutureExt;
use futures::future::{self, Either};
use log::{error, info};
use std::env;
use structopt::StructOpt;

fn show_error(e: &failure::Error) {
    error!("There was an error");
    eprint!("error");
    for cause in e.iter_chain() {
        eprint!(": {}", cause);
    }
    eprintln!("");
    //std::process::exit(1);
}

fn main() {
    init_logger();
    let _ = run();
}

fn init_logger() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info")
    }
    env_logger::init()
}

// TODO proper exit code
// TODO ctrlc
fn run() -> Fallible<()> {
    let opt = Opt::from_args();
    let config = JobConfig::from_file(&opt.jobconfig).context("reading job config")?;

    let progname = config.progname.clone();
    let cpus_requested = opt.numproc;

    System::run(move || {
        Arbiter::spawn(
            SessionMPI::init(opt.hub)
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
                    let deploy_prefix = if let Some(sources) = config.sources.clone() {
                        // It's safe to call unwrap here - at this point opt.jobconfig
                        // is guaranteed to be a valid filepath, which is checked by
                        // JobConfig::from_file
                        let sources_dir = opt.jobconfig.parent().unwrap().to_owned();
                        Either::A(
                            session
                                .deploy(sources_dir, sources, progname)
                                .context("deploying the sources")
                                .and_then(|depl| {
                                    let DeploymentInfo {
                                        logs,
                                        deploy_prefix,
                                    } = depl;
                                    // TODO better display of compilation logs
                                    info!("build logs:\n{:#?}", logs);
                                    Ok(Some(deploy_prefix))
                                }),
                        )
                    } else {
                        Either::B(future::ok(None))
                    };

                    future::ok(session).join(deploy_prefix).and_then(
                        move |(session, deploy_prefix)| {
                            session.exec(
                                cpus_requested,
                                config.progname,
                                config.args,
                                config.mpiargs,
                                deploy_prefix,
                            )
                        },
                    )
                })
                .and_then(|output| {
                    println!("Execution output:\n{}", output);
                    Ok(())
                })
                .map_err(|e| show_error(&e))
                .then(|fut| {
                    actix::System::current().stop();
                    fut
                }),
        );
    })
    .expect("actix error");

    Ok(())
}
