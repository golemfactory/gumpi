// use crate::failure_ext::OptionExt;
use crate::session::SessionMan;
use failure::{Fallible, ResultExt};
use std::path::PathBuf;
use std::net::SocketAddr;

pub struct SessionMPI<'a> {
    mgr: &'a SessionMan,
    progname: String,
    progdir: PathBuf,
}

impl<'a> SessionMPI<'a> {
    pub fn new(mgr: &'a SessionMan, progname: String) -> Self {
        let home = dirs::home_dir().expect("Unable to get the home dir");
        let progdir = home.join("pub").join(&progname);
        SessionMPI {
            mgr,
            progname,
            progdir,
        }
    }

    pub fn make(&self) -> Fallible<()> {
        let progdir = self.progdir.to_str().expect("progdir is invalid utf8");
        self.mgr.exec("make", &["-C", progdir])
    }

    pub fn run(&self, nproc: u32, args: &[&str]) -> Fallible<()> {
        let progpath = self
            .progdir
            .join(&self.progname)
            .to_str()
            .expect("progpath is invalid utf8");
        let npstr = nproc.to_string();
        let mpiargs = [&["-n", &npstr, &self.progname], args].concat();
        self.mgr.exec("mpirun", &mpiargs)
    }

    pub fn hostfile(&self) -> Fallible<String> {
        let peers = self.mgr.get_providers()?;
        let file_lines: Vec<_> = peers.iter().filter_map(|peer| {
            // TODO detect slots
            // TODO depend on number of procs?
            if let Some(ref addr) = peer.peer_addr {
                let addr: SocketAddr = addr.parse().expect("Invalid IP address");
                let line = format!("{} port=4222 slots=1", addr.ip());
                Some(line)
            } else {
                None
            }
        }).collect();
        Ok(file_lines.join("\n"))
    }
}
