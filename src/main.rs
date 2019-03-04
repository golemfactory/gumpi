#![warn(clippy::all)]

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
    mgr.exec(opt.numproc, config.progname, config.args, config.mpiargs)?;

    Ok(())
}
