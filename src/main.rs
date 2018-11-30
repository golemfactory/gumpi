extern crate failure;
extern crate reqwest;
extern crate serde;
extern crate structopt;
#[macro_use]
extern crate serde_derive;
extern crate gu_net;
extern crate serde_json;
extern crate toml;

#[macro_use]
extern crate log;

mod failure_ext;
mod jobconfig;
mod session;

use crate::{
    jobconfig::{JobConfig, Opt},
    session::{mpi::SessionMPI, SessionMan},
};
use failure::{format_err, Fallible, ResultExt};
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

    let mut mgr = SessionMan::new(opt.hub)?;
    let providers = mgr.get_provider_info().context("getting provider info")?;
    println!("PROVIDERS:");
    for p in &providers {
        println!("{:?}", p);
    }
    if providers.is_empty() {
        return Err(format_err!("No providers available"));
    }

    let mpi_sess = SessionMPI::new(&mut mgr, config.progname, providers)?;
    println!("HOSTFILE:\n{}", mpi_sess.hostfile()?);

    mpi_sess.exec(2, &["foo"])?;

    /*   info!("Creating session");
        mgr.create().context("During create")?;

        let mpimgr = SessionMPI::new(&mgr, config.progname);
        //mpimgr.make()?;
        mpimgr.run(opt.numproc, &["foo"])?;
        println!(
            "providers {:?}",
            mgr.get_providers().context("during get_providers")?
        );
        println!("{}", mpimgr.hostfile()?);
    */
    Ok(())
}
