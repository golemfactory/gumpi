extern crate clap;
extern crate failure;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

#[macro_use]
extern crate log;

mod args;
mod backend;
mod session;
use crate::session::SessionMan;

use failure::{Fallible, ResultExt};
use std::env;

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
    let matches = args::get_parser().get_matches();
    let _progname = matches.value_of("progname").unwrap();
    let _numproc = matches.value_of("numproc").unwrap();

    let mut mgr = SessionMan::new("127.0.0.1:61621".to_owned(), "127.0.0.1:61622".to_owned());
    info!("Creating session");
    mgr.create().context("During create")?;

    mgr.exec("echo", &["foo"]).context("during exec")?;
    println!("{:?}", mgr.get_providers().context("during get_providers")?);

    info!("Destroying session");
    mgr.destroy().context("during destroy")?;
    Ok(())
}
