#![warn(clippy::all)]

mod failure_ext;
mod jobconfig;
mod session;

use crate::{
    jobconfig::{JobConfig, Opt},
    session::mpi::SessionMPI,
};
use failure::{Fallible, ResultExt};
use log::debug;
use std::env;
use structopt::StructOpt;

fn main() {
    init_logger();
    if let Err(e) = run() {
        eprint!("error");
        for cause in e.iter_chain() {
            eprint!(": {}", cause);
        }
        eprintln!("");
        std::process::exit(1);
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info")
    }
    env_logger::init()
}

fn run() -> Fallible<()> {
    let opt = Opt::from_args();
    let config = JobConfig::from_file(&opt.jobconfig).context("reading job config")?;

    let prov_filter = if opt.providers.is_empty() {
        None
    } else {
        debug!("Chosen providers: {:?}", opt.providers);
        Some(opt.providers)
    };

    let mgr = SessionMPI::init(opt.hub, opt.numproc, prov_filter)?;

    let deploy_prefix;
    if let Some(sources) = config.sources {
        // It's safe to call unwrap here - at this point opt.jobconfig
        // is guaranteed to be a valid filepath, which is checked by
        // JobConfig::from_file
        let prefix = mgr
            .deploy(opt.jobconfig.parent().unwrap(), &sources, &config.progname)
            .context("deploying the sources")?;
        deploy_prefix = Some(prefix);
    } else {
        deploy_prefix = None;
    }

    mgr.exec(
        opt.numproc,
        config.progname,
        config.args,
        config.mpiargs,
        deploy_prefix,
    )?;

    Ok(())
}
