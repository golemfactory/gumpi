//use super::{Command, ProviderSession, ResourceFormat};
use crate::{
    failure_ext::FutureExt,
    jobconfig::{BuildType, Sources},
    session::gu_client_ext::PeerHardwareQuery,
};
use failure::{format_err, ResultExt};
use futures::{
    future::{self, Either},
    prelude::*,
};
use gu_client::r#async::{Blob, HubConnection, HubSession, PeerSession};
use gu_hardware::actor::Hardware;
use gu_model::{
    envman::{Command, CreateSession, Image, ResourceFormat},
    peers::PeerInfo,
    session::HubSessionSpec,
};
use gu_net::NodeId;
use log::info;
use std::{fs, net::SocketAddr, path::PathBuf};

#[derive(Debug)]
pub struct ProviderMPI {
    session: PeerSession,
    hardware: Hardware,
    info: PeerInfo,
}

pub struct SessionMPI {
    providers: Vec<ProviderMPI>,
    hub_session: HubSession,
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
                        info!("Initialized gumpi");
                        Ok(Self {
                            hub_session,
                            providers,
                        })
                    })
            },
        ))
    }

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
                let cpus = peer.hardware.num_cores();

                format!("{} port=4222 slots={}", ip, cpus)
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
        use gu_client::error::Error as GUError;

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

        let upload_cmd = Command::WriteFile {
            content: hostfile,
            file_path: "hostfile".to_owned(),
        };

        info!("Executing mpirun with args {:?}...", cmdline);

        let exec_cmd = Command::Exec {
            executable: "mpirun".to_owned(),
            args: cmdline,
        };

        root.session
            .update(vec![upload_cmd, exec_cmd])
            .map_err(|e| match e {
                GUError::ProcessingResult(outs) => {
                    assert_eq!(outs.len(), 2);

                    if outs[0] == "OK" {
                        format_err!("Execution error:\n{}", outs[1])
                    } else {
                        format_err!("WriteFile failed: {}", outs[0])
                    }
                }
                x => x.into(),
            })
            .and_then(|mut outs| {
                // outs should be a vector of length 2, of form ["OK", execution_output]
                // only the latter is interesting to us
                Ok(outs.swap_remove(1))
            })
    }

    // Returns: the deployment prefix
    pub fn deploy(
        &self,
        config_path: PathBuf,
        sources: Sources,
        progname: String,
    ) -> impl Future<Item = DeploymentInfo, Error = failure::Error> {
        let app_path = "app".to_owned();
        let tarball_path = config_path.join(&sources.path);

        // TODO read the file asynchronously
        // TODO create the tarball on the fly?
        let tarball = fs::read(tarball_path).map(Into::into);
        let tarball_stream = futures::stream::once(tarball);

        let deployments: Vec<_> = self
            .providers
            .iter()
            .map(|provider| provider.session.clone())
            .collect();

        self.hub_session
            .new_blob()
            .from_err()
            .and_then(move |blob| {
                blob.upload_from_stream(tarball_stream)
                    .from_err()
                    .and_then(move |_| Ok(blob))
            })
            .and_then(move |blob| {
                let cmds = generate_deployment_cmds(app_path, blob, progname, sources.mode);
                let build_futs = deployments
                    .into_iter()
                    .map(move |session| {
                        let node = session.node_id();
                        session
                            .update(cmds.clone())
                            .context(format!("compiling the app on node {}", node.to_string()))
                            .and_then(move |logs| Ok(CompilationInfo { logs, node }))
                    })
                    .collect::<Vec<_>>();

                future::join_all(build_futs).and_then(|logs| {
                    Ok(DeploymentInfo {
                        logs,
                        deploy_prefix: "/tmp/".to_owned(),
                    })
                })
            })
    }
}

pub struct DeploymentInfo {
    pub logs: Vec<CompilationInfo>,
    pub deploy_prefix: String,
}

#[derive(Debug)]
pub struct CompilationInfo {
    pub node: NodeId,
    pub logs: Vec<String>,
}

/// app_path: the directory where the app sources should reside
///             on the provider side
fn generate_deployment_cmds(
    app_path: String,
    blob: Blob,
    progname: String,
    mode: BuildType,
) -> Vec<Command> {
    let download_cmd = Command::DownloadFile {
        format: ResourceFormat::Tar,
        uri: blob.uri(),
        file_path: app_path.clone(),
    };

    // If we create a session per provider, every session
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
            "-DEXECUTABLE_OUTPUT_PATH=tmp".to_owned(),
        ],
    };

    let mv_cmd = Command::Exec {
        executable: "mv".to_owned(),
        args: vec![[app_path, progname].join("/"), "tmp/".to_owned()],
    };
    let make_cmd = Command::Exec {
        executable: "make".to_owned(),
        args: vec![],
    };

    match mode {
        BuildType::Make => vec![download_cmd, make_cmd, mv_cmd],
        BuildType::CMake => vec![download_cmd, cmake_cmd, make_cmd],
    }
}
