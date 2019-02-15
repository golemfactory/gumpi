use super::{Command, HubSession, ProviderSession};
use failure::{format_err, Fallible};
use log::{info, warn};
use std::net::SocketAddr;
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

    fn root_provider(&self) -> &ProviderSession {
        &self.provider_sessions[0]
    }

    pub fn hostfile(&self) -> Fallible<String> {
        let peers = &self.provider_sessions;
        let file_lines: Vec<_> = peers
            .iter()
            .filter_map(|peer| {
                // TODO depend on number of procs?
                let hw = match peer.get_hardware() {
                    Ok(hw) => hw,
                    Err(e) => {
                        warn!("Error getting hardware for peer {:?}: {}", peer, e);
                        return None;
                    }
                };

                // Use map to handle ip-less peers
                let ip_sock = &peer.peerinfo.peer_addr;
                let ip_sock: SocketAddr = ip_sock
                    .parse()
                    .unwrap_or_else(|_| panic!("GU returned an invalid IP address, {}", ip_sock));
                let ip = ip_sock.ip();
                Some(format!("{} port=4222 slots={}", ip, hw.num_cores))
            })
            .collect();
        Ok(file_lines.join("\n"))
    }

    pub fn exec<T: Into<String>>(
        &self,
        nproc: u32,
        progname: T,
        args: Vec<T>,
        mpiargs: Option<Vec<T>>,
    ) -> Fallible<()> {
        let root = self.root_provider();
        let mut cmdline = vec![];

        if let Some(args) = mpiargs {
            cmdline.extend(args.into_iter().map(T::into));
        }
        cmdline.extend(vec![
            "-n".to_owned(),
            nproc.to_string(),
            "--hostfile".to_owned(),
            "hostfile".to_owned(),
            progname.into(),
        ]);
        cmdline.extend(args.into_iter().map(T::into));

        let hostfile = self.hostfile()?;
        let blob_id = self.hub_session.upload(hostfile)?;
        info!("Downloading the hostfile...");
        let download_output = root.download(blob_id, "hostfile".to_owned());
        info!("Downloaded: {:?}", download_output);

        info!("Executing mpirun with args {:?}...", cmdline);
        let exec_cmd = Command::Exec {
            executable: "mpirun".to_owned(),
            args: cmdline,
        };

        let ret = root.exec_commands(vec![exec_cmd])?;
        println!("Output:");
        for out in ret {
            println!("{}\n========================", out);
        }
        Ok(())
    }
}

// TODO allow specifying build mode in the config
/*#[derive(PartialEq)]
pub enum BuildMode {
    Makefile,
    CMake,
}*/
