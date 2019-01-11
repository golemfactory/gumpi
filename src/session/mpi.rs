use super::gu_struct::Hardware;
use crate::session::{Command, HubSession, NodeId, ProviderSession};
use failure::{format_err, Fallible, ResultExt};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::rc::Rc;

pub struct SessionMPI {
    provider_sessions: Vec<ProviderSession>,
    hub_session: Rc<HubSession>,
}

impl SessionMPI {
    pub fn init(hub_ip: SocketAddr) -> Fallible<Self> {
        let hub_session = HubSession::new(hub_ip)?;
        let hub_session = Rc::new(hub_session);

        let providers = hub_session.get_providers()?;
        let provider_sessions: Vec<ProviderSession> = providers
            .into_iter()
            .filter_map(|p| {
                let sess = ProviderSession::new(Rc::clone(&hub_session), p);
                match sess {
                    Err(e) => {
                        warn!("Error initalizing provider session: {}", e);
                        None
                    }
                    Ok(r) => {
                        info!("Connected to provider: {:#?}", r);
                        Some(r)
                    }
                }
            })
            .collect();

        if provider_sessions.is_empty() {
            return Err(format_err!("No providers available"));
        }

        info!("Initialized GUMPI.");

        Ok(Self {
            hub_session,
            provider_sessions,
        })
    }

    /*pub fn exec_commands(&self, cmd: Vec<Command>) -> Fallible<Vec<String>> {
        let session = self.get_provider_session()?;
        let service = 38;
        let payload = SessionUpdate {
            session_id: session.session_id.clone(),
            commands: cmd,
        };
        let reply: Vec<String> = self
            .post_provider(session.node_id, service, &payload)
            .context("Command execution")?;
        Ok(reply)
    }*/

    /*pub fn get_hardware(&self) -> Vec<Hardware> {
        self.provider_sessions[0]
    }*/
}

// TODO allow specifying build mode in the config
#[allow(dead_code)]
#[derive(PartialEq)]
pub enum BuildMode {
    Makefile,
    CMake,
}
/*
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
*/
