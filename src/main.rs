#![warn(clippy::all)]
#![warn(rust_2018_idioms)]
mod failure_ext;
mod jobconfig;
mod session;

use crate::{
    jobconfig::{JobConfig, Opt},
    session::mpi::SessionMPI,
};
use actix::prelude::*;
use failure::{Fallible, ResultExt};
use futures::{future, prelude::*};
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

    System::run(move || {
        Arbiter::spawn(
            SessionMPI::init(opt.hub, opt.numproc)
                .map_err(|e| {
                    println!("Error initializing session: {}", e);
                    e
                })
                .and_then(|session| {
                    println!("providers: {:#?}", session.providers);
                    session.hub_session.list_peers().from_err()
                })
                .and_then(|peers| {
                    println!("listing session peers");
                    peers.for_each(|peer| println!("peer_id={:#?}", peer.node_id));
                    future::ok(())
                })
                .map_err(|e| {
                    println!("Error while listing peers: {:#?}.", e);
                })
                .then(|_| {
                    actix::System::current().stop();
                    Ok(())
                }),
        );
    });

    Ok(())
}

/*fn run() -> Fallible<()> {
    let opt = Opt::from_args();
    let config = JobConfig::from_file(&opt.jobconfig).context("reading job config")?;

    let mgr = SessionMPI::init(opt.hub, opt.numproc)?;

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
}*/
