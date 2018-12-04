// use crate::failure_ext::OptionExt;
use crate::session::{Command, Provider, SessionMan};
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
        mgr.init_provider_session(root.id)
            .context("creating the session failed")?;

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

    pub fn exec<T: Into<String>>(&self, nproc: u32, args: Vec<T>) -> Fallible<()> {
        let args = args.into_iter().map(T::into);
        let mut cmdline = vec!["-n".to_owned(), nproc.to_string(), self.progname.clone()];
        cmdline.extend(args);

        let hostfile = self.hostfile()?;
        let blob_id = self.mgr.upload(hostfile)?;

        let download_cmd = Command::DownloadFile {
            uri: format!(
                "http://{}/sessions/{}/blob/{}",
                self.mgr.hub_ip, self.mgr.hub_session.session_id, blob_id
            ),
            file_path: "hostfile".to_owned(),
        };
        let exec_cmd = Command::Exec {
            executable: "mpirun".to_owned(),
            args: cmdline,
        };
        let ret = self.mgr.exec_commands(vec![download_cmd, exec_cmd])?;
        println!("Output: {:?}", ret);
        Ok(())
    }

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
