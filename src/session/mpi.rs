//use super::{Command, ProviderSession, ResourceFormat};
use crate::{
    failure_ext::FutureExt,
    jobconfig::{BuildType, Sources},
    session::gu_client_ext::PeerHardwareQuery,
};
use failure::{Fallible, ResultExt};
use futures::{
    future::{self, Either},
    prelude::*,
};
use gu_client::r#async::{HubConnection, HubSession, PeerSession};
use gu_hardware::actor::Hardware;
use gu_model::{
    envman::{Command, CreateSession, Image},
    peers::PeerInfo,
    session::HubSessionSpec,
};
use log::info;
use std::{net::SocketAddr, path::Path};

#[derive(Debug)]
pub struct ProviderMPI {
    session: PeerSession,
    hardware: Hardware,
    info: PeerInfo,
}

pub struct SessionMPI {
    pub providers: Vec<ProviderMPI>,
    pub hub_session: HubSession, // TODO private
}

const GUMPI_IMAGE_URL: &str = "http://52.31.143.91/dav/gumpi-image-test.hdi";
const GUMPI_IMAGE_SHA1: &str = "367c891fb2fc603ab36fae67e8cfe1d1e8c28ff8";

impl SessionMPI {
    pub fn init(hub_ip: SocketAddr) -> impl Future<Item = SessionMPI, Error = failure::Error> {
        println!("initializing");
        let hub_conn = HubConnection::from_addr(hub_ip.to_string()).context("invalid hub address");
        let hub_conn = match hub_conn {
            Err(e) => return Either::A(future::err(e.into())),
            Ok(conn) => conn,
        };

        let hub_session = hub_conn.new_session(HubSessionSpec::default());
        let peers = hub_conn.list_peers();

        let peer_session_spec = CreateSession {
            env_type: "hd".to_owned(),
            image: Image {
                url: GUMPI_IMAGE_URL.to_owned(),
                hash: format!("SHA1:{}", GUMPI_IMAGE_SHA1),
            },
            name: "gumpi".to_owned(),
            tags: vec![],
            note: None,
            options: (),
        };

        Either::B(hub_session.join(peers).context("adding peers").and_then(
            move |(session, peers)| {
                // TODO manual cleanup
                let hub_session = session.into_inner().unwrap();
                let peers_session = hub_session.clone();

                let peers: Vec<_> = peers.collect();
                info!("peers available: {:?}", peers);
                let nodes: Vec<_> = peers.iter().map(|p| &p.node_id).cloned().collect();

                hub_session
                    .add_peers(nodes)
                    .from_err()
                    .and_then(move |nodes| {
                        let peer_sessions =
                            nodes.into_iter().zip(peers).map(move |(node_id, info)| {
                                let peer = peers_session.peer(node_id);
                                let hardware = peer.hardware().context("getting hardware info");
                                let sess = peer
                                    .new_session(peer_session_spec.clone())
                                    .context("creating peer session");
                                Future::join(sess, hardware).and_then(|(session, hardware)| {
                                    Ok(ProviderMPI {
                                        session,
                                        hardware,
                                        info,
                                    })
                                })
                            });
                        future::join_all(peer_sessions)
                    })
                    .and_then(|providers| {
                        Ok(Self {
                            hub_session,
                            providers,
                        })
                    })
            },
        ))
    }

    /*
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
    */

    fn root_provider(&self) -> &ProviderMPI {
        self.providers.first().expect("no providers")
    }

    pub fn hostfile(&self) -> String {
        let peers = &self.providers;
        let file_lines: Vec<_> = peers
            .iter()
            .map(|peer| {
                let ip_sock = &peer.info.peer_addr;
                let ip_sock: SocketAddr = ip_sock
                    .parse()
                    .unwrap_or_else(|_| panic!("GU returned an invalid IP address, {}", ip_sock));
                let ip = ip_sock.ip();

                format!("{} port=4222 slots={}", ip, 1) // TODO use peer.hardware.num_cores
            })
            .collect();
        file_lines.join("\n")
    }

    pub fn total_cpus(&self) -> usize {
        self.providers.iter().map(|p| p.hardware.num_cores()).sum()
    }

    pub fn exec<T: Into<String>>(
        &self,
        nproc: usize,
        progname: T,
        args: Vec<T>,
        mpiargs: Option<Vec<T>>,
        deploy_prefix: Option<String>,
    ) -> impl Future<Item = String, Error = failure::Error> {
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

        let hostfile = self.hostfile();
        info!("HOSTFILE:\n{}", hostfile);
        // let hostfile_stream = stream::once::<_, actix_web::Error>(Ok(hostfile.into())) ;

        /*self.hub_session
        .new_blob()
        .and_then(|blob| blob.upload_from_stream(hostfile_stream).from_err())
        .from_err()*/

        /*info!("Downloading the hostfile...");
        let download_output = root.download(blob_id, "hostfile".to_owned(), ResourceFormat::Raw);
        info!("Downloaded: {:?}", download_output);*/

        info!("Executing mpirun with args {:?}...", cmdline);
        let exec_cmd = Command::Exec {
            executable: "mpirun".to_owned(),
            args: cmdline,
        };
        root.session
            .update(vec![exec_cmd])
            .and_then(|mut outs| Ok(outs.swap_remove(0)))
            .from_err()
    }

    // FIXME lifetimes are broken
    // Returns: the deployment prefix
    pub fn deploy(
        &self,
        config_path: &Path,
        sources: &Sources,
        progname: &str,
    ) -> impl Future<Item = Vec<Vec<String>>, Error = failure::Error> {
        let app_path = "app".to_owned();
        let tarball_path = config_path.join(&sources.path);

        /*let blob_id = self
        .hub_session
        .upload_file(&tarball_path)
        .context("uploading file")?;*/

        let build_futs = self.providers.iter().map(|provider| {
            /*provider
            .download(blob_id, app_path.clone(), ResourceFormat::Tar)
            .context("downloading file")?;*/

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

            provider
                .session
                .update(cmds)
                .context(format!("compiling the app on node {:?}", provider))
        });
        let build = future::join_all(build_futs);
        // FIXME return build instead of a dummy thing
        future::ok(vec![vec![]])
        //Ok("/tmp/".to_owned())
    }
}
