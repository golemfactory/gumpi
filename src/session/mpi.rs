// use crate::failure_ext::OptionExt;
use crate::session::{Command, Provider, SessionMan};
use failure::{Fallible, ResultExt};

pub struct SessionMPI<'a> {
    mgr: &'a mut SessionMan,
    progname: String,
    providers: Vec<Provider>,
}

impl<'a> SessionMPI<'a> {
    pub fn new(
        mgr: &'a mut SessionMan,
        progname: String,
        providers: Vec<Provider>,
    ) -> Fallible<Self> {
        // TODO check if providers is not empty
        let root = &providers[0];
        mgr.init_provider_session(root.id)
            .context("creating the session failed")?;

        Ok(SessionMPI {
            mgr,
            progname,
            providers,
        })
    }

    /*pub fn make(&self) -> Fallible<()> {
        let progdir = self.progdir.to_str().expect("progdir is invalid utf8");
        self.mgr.exec("make", &["-C", progdir])
    }*/

    pub fn exec<T: Into<String>>(&self, nproc: u32, args: Vec<T>) -> Fallible<()> {
        let args = args.into_iter().map(T::into);
        let mut cmdline = vec![
            "-n".to_owned(),
            nproc.to_string(),
            "--hostfile".to_owned(),
            "hostfile".to_owned(),
            self.progname.clone(),
        ];
        cmdline.extend(args);

        let hostfile = self.hostfile()?;
        let blob_id = self.mgr.upload(hostfile)?;

        let download_cmd = Command::DownloadFile {
            uri: format!(
                "http://{}/sessions/{}/blobs/{}",
                self.mgr.hub_ip, self.mgr.hub_session.session_id, blob_id
            ),
            file_path: "hostfile".to_owned(),
        };
        let exec_cmd = Command::Exec {
            executable: "mpirun".to_owned(),
            args: cmdline,
        };
        info!("Executing...");
        let ret = self.mgr.exec_commands(vec![download_cmd, exec_cmd])?;
        println!("Output:");
        for out in ret {
            println!("{}\n========================", out);
        }
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
