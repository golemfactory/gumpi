use super::{Command, HubSession, ProviderSession, ResourceFormat};
use crate::jobconfig::{BuildType, Sources};
use failure::{format_err, Fallible, ResultExt};
use log::{info, warn};
use std::{net::SocketAddr, path::Path, rc::Rc};

pub struct SessionMPI {
    provider_sessions: Vec<ProviderSession>,
    hub_session: Rc<HubSession>,
}

impl SessionMPI {
    pub fn init(hub_ip: SocketAddr, cpus_requested: usize) -> Fallible<Self> {
        if hub_ip.ip().is_loopback() {
            warn!(
                "The hub address {} is a loopback address. \
                 This is discouraged and you may experience connectivity problems. \
                 See issue #37.",
                hub_ip
            );
        }

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

        let cpus_available: usize = provider_sessions
            .iter()
            .map(|peer| peer.hardware.num_cores)
            .sum();
        if cpus_available < cpus_requested {
            return Err(format_err!(
                "Not enough CPUs available: requested: {}, available: {}",
                cpus_requested,
                cpus_available
            ));
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
            .map(|peer| {
                let ip_sock = &peer.peerinfo.peer_addr;
                let ip_sock: SocketAddr = ip_sock
                    .parse()
                    .unwrap_or_else(|_| panic!("GU returned an invalid IP address, {}", ip_sock));
                let ip = ip_sock.ip();

                format!("{} port=4222 slots={}", ip, peer.hardware.num_cores)
            })
            .collect();
        Ok(file_lines.join("\n"))
    }

    pub fn exec<T: Into<String>>(
        &self,
        nproc: usize,
        progname: T,
        args: Vec<T>,
        mpiargs: Option<Vec<T>>,
        deploy_prefix: Option<String>,
    ) -> Fallible<()> {
        let root = self.root_provider();
        let mut cmdline = vec![];

        if let Some(args) = mpiargs {
            cmdline.extend(args.into_iter().map(T::into));
        }

        // We've moved the executable to /tmp in deploy, so now correct the path
        // to reflect this change.
        let progname = progname.into();
        let progname = deploy_prefix.map(|p| p + &progname).unwrap_or(progname);

        cmdline.extend(vec![
            "-n".to_owned(),
            nproc.to_string(),
            "--hostfile".to_owned(),
            "hostfile".to_owned(),
            progname,
        ]);
        cmdline.extend(args.into_iter().map(T::into));

        let hostfile = self.hostfile()?;
        info!("HOSTFILE:\n{}", hostfile);
        let blob_id = self.hub_session.upload(hostfile)?;
        info!("Downloading the hostfile...");
        let download_output = root.download(blob_id, "hostfile".to_owned(), ResourceFormat::Raw);
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

    // Returns: the deployment prefix
    pub fn deploy(
        &self,
        config_path: &Path,
        sources: &Sources,
        progname: &str,
    ) -> Fallible<String> {
        let app_path = "app".to_owned();
        let tarball_path = config_path.join(&sources.path);

        let blob_id = self
            .hub_session
            .upload_file(&tarball_path)
            .context("uploading file")?;

        for provider in &self.provider_sessions {
            provider
                .download(blob_id, app_path.clone(), ResourceFormat::Tar)
                .context("downloading file")?;

            // If we create a ProviderSession per provider, every session
            // gets a unique identifier. This means that the resulting executable
            // resides in a different directory on each of the provider nodes,
            // which causes mpirun to fail.
            // As a workaround, we provide a symlink to the /tmp directory
            // in the image and put the resulting binary there.

            // For the CMake backend we use the EXECUTABLE_OUTPUT_PATH CMake variable
            // For the Make backend we just move the file around

            let cmake_cmd = Command::Exec {
                executable: "cmake/bin/cmake".to_owned(),
                args: vec![
                    app_path.clone(),
                    "-DCMAKE_C_COMPILER=mpicc".to_owned(),
                    "-DCMAKE_CXX_COMPILER=mpicxx".to_owned(),
                    "-DCMAKE_BUILD_TYPE=Release".to_owned(),
                    "-DEXECUTABLE_OUTPUT_PATH=tmp".to_owned(), // TODO fix path for Make
                ],
            };
            let mv_cmd = Command::Exec {
                executable: "mv".to_owned(),
                args: vec![[&app_path, progname].join("/"), "tmp/".to_owned()],
            };
            let make_cmd = Command::Exec {
                executable: "make".to_owned(),
                args: vec![],
            };

            let cmds = match &sources.mode {
                BuildType::Make => vec![make_cmd, mv_cmd],
                BuildType::CMake => vec![cmake_cmd, make_cmd],
            };

            let out = provider
                .exec_commands(cmds)
                .context(format!("compiling the app on node {}", provider.name()))?;
            for out in out {
                info!("Provider {} compilation output:\n{}", provider.name(), out);
            }
        }
        Ok("/tmp/".to_owned())
    }
}
