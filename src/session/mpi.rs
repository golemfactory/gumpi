// use crate::failure_ext::OptionExt;
use crate::session::{Provider, SessionMan};
use failure::{Fallible, ResultExt};
use std::path::PathBuf;

pub struct SessionMPI<'a> {
    mgr: &'a mut SessionMan,
    progname: String,
    progdir: PathBuf,
    providers: Vec<Provider>,
}

impl<'a> SessionMPI<'a> {
    pub fn new(
        mgr: &'a mut SessionMan,
        progname: String,
        providers: Vec<Provider>,
    ) -> Fallible<Self> {
        let home = dirs::home_dir().expect("Unable to get the home dir");
        let progdir = home.join("pub").join(&progname);
        // TODO check if providers is not empty
        let root = &providers[0];
        mgr.create(root.id).context("creating the session failed")?;

        Ok(SessionMPI {
            mgr,
            progname,
            progdir,
            providers,
        })
    }

    /*pub fn make(&self) -> Fallible<()> {
        let progdir = self.progdir.to_str().expect("progdir is invalid utf8");
        self.mgr.exec("make", &["-C", progdir])
    }*/

    /*pub fn run(&self, nproc: u32, args: &[&str]) -> Fallible<()> {
        let progpath = self
            .progdir
            .join(&self.progname)
            .to_str()
            .expect("progpath is invalid utf8");
        let npstr = nproc.to_string();
        let mpiargs = [&["-n", &npstr, &self.progname], args].concat();
        self.mgr.exec("mpirun", &mpiargs)
    }*/

    pub fn hostfile(&self) -> Fallible<String> {
        let peers = &self.providers;
        let file_lines: Vec<_> = peers
            .iter()
            .map(|peer| {
                // TODO depend on number of procs?
                format!("{} port=4222 slots={}", peer.ip, peer.cpus)
            })
            .collect();
        Ok(file_lines.join("\n"))
    }
}
