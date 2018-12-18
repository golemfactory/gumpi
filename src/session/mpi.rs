use crate::session::{Command, Provider, SessionMan};
use failure::{Fallible, ResultExt};
use std::path::Path;

// TODO allow specifying build mode in the config
#[allow(dead_code)]
#[derive(PartialEq)]
pub enum BuildMode {
    Makefile,
    CMake,
}

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

    pub fn exec<T: Into<String>>(
        &self,
        nproc: u32,
        args: Vec<T>,
        mpiargs: Option<Vec<T>>,
    ) -> Fallible<()> {
        let mut cmdline = vec![];

        if let Some(args) = mpiargs {
            cmdline.extend(args.into_iter().map(T::into));
        }
        cmdline.extend(vec![
            "-n".to_owned(),
            nproc.to_string(),
            "--hostfile".to_owned(),
            "hostfile".to_owned(),
            self.progname.clone(),
        ]);
        cmdline.extend(args.into_iter().map(T::into));

        let hostfile = self.hostfile()?;
        let blob_id = self.mgr.upload(hostfile)?;

        let download_cmd = self.mgr.get_download_cmd(blob_id, "hostfile".to_owned());
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

    pub fn build(&self, sources_archive: &Path, mode: BuildMode) -> Fallible<()> {
        let filename = sources_archive
            .file_name()
            .expect("Invalid path for sources")
            .to_str()
            .expect("Invalid filename");
        let blob_id = self.mgr.upload_file(sources_archive)?;
        let download_cmd = self.mgr.get_download_cmd(blob_id, filename.to_owned());

        let unpack_cmd = Command::Exec {
            executable: "unzip".to_owned(),
            args: vec![filename.to_owned()],
        };

        let mut cmds = vec![download_cmd, unpack_cmd];

        if mode == BuildMode::CMake {
            cmds.push(Command::Exec {
                executable: "cmake".to_owned(),
                args: vec![".".to_owned()],
            });
        }

        cmds.push(Command::Exec {
            executable: "make".to_owned(),
            args: vec![],
        });

        let ret = self.mgr.exec_commands(cmds)?;
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
