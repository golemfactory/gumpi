#![warn(clippy::all)]

mod actix;
mod failure_ext;
mod jobconfig;
mod session;

use crate::{
    jobconfig::{JobConfig, Opt},
    session::mpi::SessionMPI,
};
use failure::{Fallible, ResultExt};
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

    let mgr = SessionMPI::init(opt.hub)?;

    if let Some(sources) = config.sources {
        // It's safe to call unwrap here - at this point opt.jobconfig
        // is guaranteed to be a valid filepath, which is checked by
        // JobConfig::from_file
        mgr.deploy(opt.jobconfig.parent().unwrap(), &sources)
            .context("deploying the sources")?;
    }
    mgr.exec(opt.numproc, config.progname, config.args, config.mpiargs)?;
    if let Some(output_path) = config.output_path {
        mgr.retrieve_output(&output_path)?;
    }

    Ok(())
}
