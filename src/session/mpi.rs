//use super::{Command, ProviderSession, ResourceFormat};
use crate::{
    error::Error,
    jobconfig::{BuildType, OutputConfig, Sources},
    session::gu_client_ext::PeerHardwareQuery,
};
use actix_web::{client, HttpMessage};
use failure::{format_err, ResultExt};
use failure_ext::{FutureExt, OptionExt};
use futures::{
    future::{self, Either},
    prelude::*,
};
use gu_client::error::Error as GUError;

use gu_client::{
    model::{
        envman::{Command, CreateSession, Image, ResourceFormat},
        peers::PeerInfo,
        session::HubSessionSpec,
    },
    r#async::{Blob, HubConnection, HubSession, PeerSession},
    NodeId,
};
use gu_hardware::actor::Hardware;
use log::{debug, info, warn};
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

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

const GUMPI_IMAGE_URL: &str = "http://52.31.143.91/dav/gumpi-image.hdi";
const GUMPI_IMAGE_SHA1: &str = "e50575bb61c20b716e89a307264bdb6e5e981919";

impl SessionMPI {
    pub fn init(
        hub_ip: SocketAddr,
        prov_filter: Option<Vec<NodeId>>,
    ) -> impl Future<Item = SessionMPI, Error = failure::Error> {
        println!("initializing gumpi");
        if hub_ip.ip().is_loopback() {
            warn!(
                "The hub address {} is a loopback address. \
                 This is discouraged and you may experience connectivity problems. \
                 See issue #37.",
                hub_ip
            );
        }

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
                let hub_session = session.into_inner().unwrap();
                let peers_session = hub_session.clone();

                let peers: Vec<_> = peers.collect();
                info!("peers available: {:#?}", peers);
                let chosen_peers: Vec<_> = peers
                    .iter()
                    .filter(|p| {
                        let node_id = &p.node_id;
                        // If the user wants to filter the providers, do it
                        let remains = prov_filter
                            .as_ref()
                            .map(|provs| provs.contains(node_id))
                            .unwrap_or(true);

                        if !remains {
                            info!("Ignoring provider: {}", node_id.to_string());
                        };

                        remains
                    })
                    .cloned()
                    .collect();
                let nodes: Vec<_> = chosen_peers.iter().map(|p| p.node_id).collect();

                hub_session
                    .add_peers(nodes)
                    .from_err()
                    .and_then(move |_| {
                        let peer_sessions = chosen_peers.into_iter().map(move |info| {
                            let node_id = info.node_id;
                            info!("Connecting to peer {}", node_id.to_string());
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

    pub fn close(&self) -> impl Future<Item = (), Error = GUError> {
        self.hub_session.clone().delete().and_then(|()| {
            info!("Session closed");
            Ok(())
        })
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
                GUError::ProcessingResult(mut outs) => {
                    assert_eq!(outs.len(), 2);

                    if outs[0] == "OK" {
                        Error::ExecutionError(outs.swap_remove(1)).into()
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

    /// Returns: the deployment prefix
    ///
    /// The resulting executable may not reside in the root of the GU image.
    /// The deployment prefix describes the folder where the application has
    /// been deployed. With the current golem-unlimited design, this is
    /// relative to the image root.
    ///
    /// See the comments in generate_deployment_cmds for more details.
    pub fn deploy(
        &self,
        config_path: PathBuf,
        sources: Sources,
        progname: String,
    ) -> impl Future<Item = DeploymentInfo, Error = failure::Error> {
        let app_path = "app".to_owned();
        let tarball_path = config_path.join(&sources.path);

        let deployments: Vec<_> = self
            .providers
            .iter()
            .map(|provider| provider.session.clone())
            .collect();

        self.upload_to_hub(&tarball_path)
            .context("uploading the source tarball")
            .and_then(move |blob| {
                let cmds = generate_deployment_cmds(app_path, blob, progname, sources.mode);
                info!("Building the application on provider nodes");
                debug!("Executing the following build commands: {:#?}", cmds);
                let build_futs = deployments
                    .into_iter()
                    .map(move |session| {
                        let node = session.node_id();
                        session
                            .update(cmds.clone())
                            .map_err(|e| -> failure::Error {
                                match e {
                                    GUError::ProcessingResult(outs) => {
                                        Error::CompilationError(outs).into()
                                    }
                                    x => x.into(),
                                }
                            })
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

    fn upload_to_hub(&self, file_path: &Path) -> impl Future<Item = Blob, Error = GUError> {
        let fname = file_path.to_string_lossy().into_owned();
        let file = fs::read(file_path).map(Into::into);
        let file_stream = futures::stream::once(file);
        self.hub_session
            .new_blob()
            .from_err()
            .and_then(move |blob| {
                info!("Uploading the file {} to the hub", fname);
                blob.upload_from_stream(file_stream)
                    .and_then(move |_| Ok(blob))
            })
    }

    pub fn upload_input(
        &self,
        input_tarball: PathBuf,
    ) -> impl Future<Item = (), Error = failure::Error> {
        let root_session = self.root_provider().session.clone();
        let name = input_tarball
            .file_name()
            .ok_or_else(|| format_err!("input_tarball is not a file"))
            .and_then(|s| {
                s.to_str()
                    .ok_or_context("invalid UTF-8")
                    .map(str::to_owned)
                    .map_err(Into::into)
            });

        self.upload_to_hub(&input_tarball)
            .context("uploading input data")
            .join(name.into_future())
            .and_then(move |(blob, name)| {
                info!("Downloading the input data, {}", name);

                let download_cmd = Command::DownloadFile {
                    format: ResourceFormat::Tar,
                    uri: blob.uri(),
                    file_path: "".to_owned(),
                };
                root_session.update(vec![download_cmd]).from_err()
            })
            .and_then(|_| Ok(()))
    }

    pub fn retrieve_output(
        &self,
        output_cfg: &OutputConfig,
    ) -> impl Future<Item = (), Error = failure::Error> {
        let path = output_cfg
            .source
            .to_str()
            .ok_or_context("output_path is not valid unicode")
            .map(str::to_owned)
            .into_future()
            .map_err(failure::Error::from);
        let output_file = output_cfg.target.clone();
        let blob = self.hub_session.new_blob().from_err();
        let root_session = self.root_provider().session.clone();

        blob.join(path)
            .and_then(move |(blob, path)| {
                info!("Uploading the outputs from the provider to the hub");
                let cmd = Command::UploadFile {
                    file_path: path,
                    format: ResourceFormat::Tar,
                    uri: blob.uri(),
                };
                root_session
                    .update(vec![cmd])
                    .context("uploading the outputs from the provider to the hub")
                    .and_then(|_| future::ok(blob))
            })
            .and_then(|blob| {
                info!("Downloading the outputs from the hub");
                client::ClientRequest::get(blob.uri())
                    .finish()
                    .unwrap()
                    .send()
                    .context("Downloading the outputs from the hub")
                    .and_then(|response| {
                        let status = response.status();
                        if status.is_success() {
                            Either::A(response.body().limit(1024 * 1024 * 1024).from_err()) // 1 GiB limit
                        } else {
                            let err = format_err!("Error downloading the outputs: {}", status);
                            Either::B(future::err(err))
                        }
                    })
            })
            .and_then(|body| {
                info!(
                    "Writing the application outputs to {}",
                    output_file.to_string_lossy()
                );
                fs::write(output_file, body)
                    .context("Writing the outputs")
                    .map_err(Into::into)
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
    match mode {
        BuildType::Make => {
            let mv_cmd = Command::Exec {
                executable: "mv".to_owned(),
                args: vec![[app_path, progname].join("/"), "tmp/".to_owned()],
            };
            let make_cmd = Command::Exec {
                executable: "make".to_owned(),
                args: vec!["-C".to_owned(), "app".to_owned()],
            };
            vec![download_cmd, make_cmd, mv_cmd]
        }
        BuildType::CMake => {
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
            let make_cmd = Command::Exec {
                executable: "make".to_owned(),
                args: vec![],
            };
            vec![download_cmd, cmake_cmd, make_cmd]
        }
    }
}
