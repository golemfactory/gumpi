//use super::{Command, ProviderSession, ResourceFormat};
use crate::{
    error::Error,
    jobconfig::{BuildType, OutputConfig, Sources},
    session::gu_client_ext::PeerHardwareQuery,
};
use actix_web::{client, HttpMessage};
use failure::{format_err, Fallible, ResultExt};
use failure_ext::{FutureExt, OptionExt};
use futures::{
    future::{self, Either},
    prelude::*,
};
use gu_client::{
    error::Error as GUError,
    model::{
        dockerman::{CreateOptions, NetDef},
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

const GUMPI_IMAGE_NAME: &str = "marmistrz/gumpi";
const GUMPI_IMAGE_VERSION: &str = "0.0.3";
const GUMPI_IMAGE_CHECKSUM: &str =
    "sha256:285b81248af0b9e0f11cfde12edc3cb149b1b74afceb43b6fea8c662d78aeaaa";
const GUMPI_ENV_TYPE: &str = "docker";

const GUMPI_DOCKER_USER: &str = "mpirun";

const APP_SOURCES_PATH: &str = "/app";
const APP_INPUT_PATH: &str = "/input";
const APP_WORKDIR: &str = "/output";

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

        let docker_image = format!("{}:{}", GUMPI_IMAGE_NAME, GUMPI_IMAGE_VERSION);
        let peer_session_spec = CreateSession {
            env_type: GUMPI_ENV_TYPE.to_owned(),
            image: Image {
                url: docker_image,
                hash: GUMPI_IMAGE_CHECKSUM.to_owned(),
            },
            name: "gumpi".to_owned(),
            tags: vec![],
            note: None,
            options: CreateOptions {
                autostart: true,
                // We need --network=host to get connectivity between containers,
                // OpenMPI uses high ports for inter-node communication
                net: Some(NetDef::Host {}),
                // The vader shared memory transport in OpenMPI uses
                //     * process_vm_readv
                //     * process_vm_writev
                // to ensure single copy. These syscalls are disabled in the default Docker
                // seccomp profile and can be enabled by the `SYS_PTRACE` capability
                // cf. https://github.com/open-mpi/ompi/issues/4948
                cap_add: vec!["SYS_PTRACE".to_owned()],
                ..CreateOptions::default()
            },
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

    pub fn exec(
        &self,
        nproc: usize,
        progname: String,
        args: Vec<String>,
        mpiargs: Vec<String>,
        deployed: bool,
    ) -> impl Future<Item = String, Error = failure::Error> {
        let root = self.root_provider();

        // Step 1: prepare the command line.
        //
        // We execute the program on the root provider in the following manner:
        //      runuser -u mpirun -- mpirun /path/to/executable arg1 arg2

        let mut cmdline = vec![];

        // We use runuser to make sure we're not running as root
        let executable = "runuser".to_owned();

        let runuser_args = vec!["-u", GUMPI_DOCKER_USER, "--"]
            .into_iter()
            .map(ToOwned::to_owned);
        cmdline.extend(runuser_args);

        // ... to call mpirun, first adding gumpi-logic arguments, later the
        // custom user defined arguments
        cmdline.push("mpirun".to_owned());
        cmdline.extend(vec![
            "-n".to_owned(),
            nproc.to_string(),
            "--hostfile".to_owned(),
            "/hostfile".to_owned(),
        ]);
        cmdline.extend(mpiargs);

        // ... then the program name ...
        //
        // If we've built the sources, we need to give the exact path to the binary
        // Otherwise it's somewhere on the system, so let the user decide
        let progname = if deployed {
            format!("{}/{}", APP_SOURCES_PATH, progname)
        } else {
            progname
        };
        cmdline.push(progname);

        // Finally the user-defined applicadtion arguments
        cmdline.extend(args);
        let cmdline = cmdline;
        info!("Executing mpirun with args {:?}...", cmdline);

        // Step 2: upload the hostfile and execute the command
        let hostfile = self.hostfile();
        info!("HOSTFILE:\n{}", hostfile);

        let upload_cmd = Command::WriteFile {
            content: hostfile,
            file_path: "hostfile".to_owned(),
        };

        let exec_cmd = Command::Exec {
            executable,
            args: cmdline,
            working_dir: APP_WORKDIR.to_owned().into(),
        };

        root.session
            .update(vec![upload_cmd, exec_cmd])
            .map_err(|e| match e {
                GUError::ProcessingResult(mut outs) => {
                    assert_eq!(outs.len(), 2);
                    use log::error;
                    error!("Processing error: {:?}", outs);

                    // This is awful and hacky, but GU is inconsistent
                    // see https://github.com/golemfactory/gumpi/issues/52
                    if outs[0].contains("OK") {
                        Error::ExecutionError(outs.swap_remove(1)).into()
                    } else {
                        format_err!("WriteFile failed: {}", outs[0])
                    }
                }
                x => x.into(),
            })
            .and_then(|mut outs| {
                // in case of successs, outs should be a vector of length 2,
                // of form [_, execution_output]
                // only the latter is interesting to us
                Ok(outs.swap_remove(1))
            })
    }

    fn get_deployments(&self) -> Vec<PeerSession> {
        self.providers
            .iter()
            .map(|provider| provider.session.clone())
            .collect()
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
    ) -> impl Future<Item = DeploymentInfo, Error = failure::Error> {
        let tarball_path = config_path.join(&sources.path);

        let deployments: Vec<_> = self.get_deployments();

        self.upload_to_hub(&tarball_path)
            .context("uploading the source tarball")
            .and_then(move |blob| {
                let cmds = generate_deployment_cmds(blob, sources.mode);
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

                future::join_all(build_futs).and_then(|logs| Ok(DeploymentInfo { logs }))
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
        let deployments = self.get_deployments();
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
                    file_path: APP_INPUT_PATH.to_owned(),
                };
                let futures = deployments
                    .into_iter()
                    .map(move |session| session.update(vec![download_cmd.clone()]).from_err());
                future::join_all(futures)
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
                            Either::A(response.body().limit(1024 * 1024 * 1024).from_err())
                        // 1 GiB limit
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

    pub fn deploy_keys(&self) -> Fallible<impl Future<Item = (), Error = failure::Error>> {
        info!("Deploying the keys");

        let (privkey, pubkey) = generate_keypair().context("generating SSH keys")?;

        let privkey_path = format!("home/{}/.ssh/id_rsa", GUMPI_DOCKER_USER);
        let pubkey_path = format!("home/{}/.ssh/id_rsa.pub", GUMPI_DOCKER_USER);
        let authorized_keys_path = format!("home/{}/.ssh/authorized_keys", GUMPI_DOCKER_USER);

        let cmds = vec![
            Command::WriteFile {
                content: privkey,
                file_path: privkey_path,
            },
            Command::WriteFile {
                content: pubkey.clone(),
                file_path: pubkey_path,
            },
            Command::WriteFile {
                content: pubkey,
                file_path: authorized_keys_path,
            },
        ];

        let futs = self
            .get_deployments()
            .into_iter()
            .map(move |session| session.update(cmds.clone()));
        let ret = future::join_all(futs)
            .map(|_| ())
            .map_err(|e| -> failure::Error {
                match e {
                    GUError::ProcessingResult(outs) => Error::KeyDeploymentError(outs).into(),
                    x => x.into(),
                }
            });
        Ok(ret)
    }
}

pub struct DeploymentInfo {
    pub logs: Vec<CompilationInfo>,
}

#[derive(Debug)]
pub struct CompilationInfo {
    pub node: NodeId,
    pub logs: Vec<String>,
}

fn generate_deployment_cmds(blob: Blob, mode: BuildType) -> Vec<Command> {
    let download_cmd = Command::DownloadFile {
        format: ResourceFormat::Tar,
        uri: blob.uri(),
        file_path: APP_SOURCES_PATH.to_owned(),
    };

    let mut commands = vec![download_cmd];
    let compile_commands = match mode {
        BuildType::Make => {
            let make_cmd = Command::Exec {
                executable: "make".to_owned(),
                args: vec!["-C".to_owned(), APP_SOURCES_PATH.to_owned()],
                working_dir: APP_SOURCES_PATH.to_owned().into(),
            };
            vec![make_cmd]
        }
        BuildType::CMake => {
            let cmake_cmd = Command::Exec {
                executable: "cmake".to_owned(),
                args: vec![
                    ".".to_owned(),
                    "-DCMAKE_C_COMPILER=mpicc".to_owned(),
                    "-DCMAKE_CXX_COMPILER=mpicxx".to_owned(),
                    "-DCMAKE_BUILD_TYPE=Release".to_owned(),
                ],
                working_dir: APP_SOURCES_PATH.to_owned().into(),
            };
            let make_cmd = Command::Exec {
                executable: "make".to_owned(),
                args: vec![],
                working_dir: APP_SOURCES_PATH.to_owned().into(),
            };
            vec![cmake_cmd, make_cmd]
        }
    };
    commands.extend(compile_commands);
    commands
}

fn generate_keypair() -> Fallible<(String, String)> {
    use openssh_keys::PublicKey;
    use openssl::rsa::Rsa;
    const SSH_RSA_KEYSIZE: u32 = 4096;

    let rsa = Rsa::generate(SSH_RSA_KEYSIZE)?;
    let privkey = rsa.private_key_to_pem()?;
    let privkey = String::from_utf8(privkey)?;

    let e = rsa.e().to_vec();
    let n = rsa.n().to_vec();
    let mut pubkey = PublicKey::from_rsa(e, n);
    pubkey.set_comment("gumpi");
    let pubkey = pubkey.to_key_format();

    Ok((privkey, pubkey))
}
