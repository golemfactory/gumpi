use crate::actix::wait_ctrlc;
use actix_web::{client::ClientRequest, http::Method, HttpMessage};
use bytes::Bytes;
use failure::{format_err, Fail, Fallible, ResultExt};
use futures::prelude::*;
use gu_model::envman::SessionUpdate;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{convert::Into, fmt, fs, net::SocketAddr, path::Path, rc::Rc, str, time::Duration};

mod gu_struct;
pub mod mpi;

use self::gu_struct::*;
pub use gu_model::envman::{Command, CreateSession, DestroySession, Image, ResourceFormat};
use gu_model::peers::PeerInfo;
use gu_net::NodeId;

#[derive(Debug)]
struct ProviderSession {
    session_id: String,
    peerinfo: PeerInfo,
    hardware: Hardware,
    hub_session: Rc<HubSession>,
}

const GUMPI_IMAGE_URL: &str = "http://52.31.143.91/dav/gumpi-image-test.hdi";
const GUMPI_IMAGE_SHA1: &str = "367c891fb2fc603ab36fae67e8cfe1d1e8c28ff8";

impl ProviderSession {
    pub fn new(hub_session: Rc<HubSession>, peerinfo: PeerInfo) -> Fallible<Self> {
        let node_id = peerinfo.node_id;
        let cs_service = 37;

        let payload = CreateSession {
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

        let session_id: String = hub_session
            .post_provider(node_id, cs_service, &payload)
            .context("Creating the provider session")?;
        info!("Session id: {}", session_id);

        let hw_service = 19354;
        let hardware = hub_session
            .post_provider(node_id, hw_service, &())
            .context("Getting hardware info")?;

        Ok(Self {
            session_id,
            hardware,
            peerinfo,
            hub_session,
        })
    }

    /// this method is private, destruction is a part of RAII
    /// if no session has been established, this is a no-op
    fn destroy(&mut self) -> Fallible<()> {
        let service = 40;

        let payload = DestroySession {
            session_id: self.session_id.clone(),
        };

        let reply: String = self
            .hub_session
            .post_provider(self.peerinfo.node_id, service, &payload)
            .context("POST request")?;
        info!("Reply: {}", reply);

        Ok(())
    }

    pub fn post_service<T, U>(&self, service: u32, json: &T) -> Fallible<U>
    where
        T: Serialize,
        for<'a> U: Deserialize<'a>,
        U: Sync + Send + 'static,
    {
        self.hub_session
            .post_provider(self.peerinfo.node_id, service, json)
    }

    fn exec_commands<I>(&self, cmds: I) -> Fallible<Vec<String>>
    where
        I: IntoIterator<Item = Command>,
    {
        let service = 38;
        let payload = SessionUpdate {
            session_id: self.session_id.clone(),
            commands: cmds.into_iter().collect(),
        };
        let reply: Vec<String> = self
            .post_service(service, &payload)
            .context("Command execution")?;
        Ok(reply)
    }

    fn exec_command(&self, cmd: Command) -> Fallible<String> {
        let mut results = self.exec_commands(vec![cmd])?;
        assert_eq!(results.len(), 1, "expected only one output of the command");
        Ok(results.swap_remove(0))
    }

    pub fn download(
        &self,
        blob_id: BlobId,
        filename: String,
        fmt: ResourceFormat,
    ) -> Fallible<String> {
        let cmd = self.hub_session.get_download_cmd(blob_id, filename, fmt);
        self.exec_command(cmd)
    }

    pub fn upload(
        &self,
        blob_url: String,
        file_path: String,
        format: ResourceFormat,
    ) -> Fallible<String> {
        let cmd = Command::UploadFile {
            file_path,
            format,
            uri: blob_url,
        };
        self.exec_command(cmd)
    }

    pub fn name(&self) -> &str {
        &self.peerinfo.peer_addr
    }
}

impl Drop for ProviderSession {
    fn drop(&mut self) {
        self.destroy()
            .expect("Destroying the provider session failed");
    }
}

#[derive(Debug)]
struct HubSession {
    session_id: u64,
    hub_ip: SocketAddr,
}

impl HubSession {
    pub fn new(hub_ip: SocketAddr) -> Fallible<Self> {
        info!("Initializing a hub session");
        let url = format!("http://{}/sessions", hub_ip);
        let payload = SessionInfo {
            name: "gumpi".to_owned(),
            environment: "hd".to_owned(),
        };
        let session_id: u64 = query_deserialize(Method::POST, &url, payload)?.expect("No content");
        let session = HubSession { session_id, hub_ip };
        Ok(session)
    }

    fn destroy(&self) -> Fallible<()> {
        let url = format!("http://{}/sessions/{}", self.hub_ip, self.session_id);
        query_deserialize::<_, ()>(Method::DELETE, &url, ())?;
        Ok(())
    }

    // When using this function, the type should be explicitly annotated!
    // This is probably a bug in the Rust compiler
    // see https://github.com/rust-lang/rust/issues/55928
    fn post_provider<T, U>(&self, provider: NodeId, service: u32, json: &T) -> Fallible<U>
    where
        T: Serialize,
        for<'a> U: Deserialize<'a>,
        U: Sync + Send + 'static,
    {
        let url = format!(
            "http://{}/peers/send-to/{}/{}",
            self.hub_ip,
            provider.to_string(),
            service
        );

        // corresponds to Envelope from gu_hub/src/peer.rs
        let payload = json!({ "b": json });

        // The provider actually returns a Result<U, _>.
        // reply will be Err(_) if an error occurred on the provider
        // side, but the HTTP request succeeded.
        // Still, if there was an error on the provider side, we want to
        // propagate the error, hence the map_err.

        // TODO HACK investigate what should the error type be and get a decent error message
        let reply: Result<U, serde_json::Value> =
            query_deserialize(Method::POST, &url, payload)?.expect("No content");
        reply.map_err(|err| format_err!("Provider replied: {}", err))
    }

    pub fn reserve_blob(&self) -> Fallible<(String, BlobId)> {
        info!("Creating a slot");
        let url = format!("http://{}/sessions/{}/blobs", self.hub_ip, self.session_id);
        let blob_id: u64 = query_deserialize(Method::POST, &url, ())?.expect("No content");
        let url = format!("{}/{}", url, blob_id);
        Ok((url, blob_id))
    }

    /// Returns: blob id
    pub fn upload(&self, payload: String) -> Fallible<BlobId> {
        let (url, blob_id) = self.reserve_blob()?;
        info!("Uploading a file, id = {}", blob_id);
        debug!("File contents: {}", payload);

        let future = ClientRequest::build()
            .method(Method::PUT)
            .timeout(Duration::from_secs(60 * 60 * 24 * 366)) // 1 year of timeout
            .uri(&url)
            .body(&payload)
            .into_future()
            .and_then(|req| req.send().from_err())
            .map_err(Into::into)
            .and_then(|resp| {
                let status = resp.status();
                if status.is_success() {
                    Ok(())
                } else {
                    Err(format_err!("Error uploading a blob: {}", status))
                }
            });

        wait_ctrlc(future).context(format!("connecting to {}", url))?;

        info!("Uploaded a file, id = {}", blob_id);
        Ok(blob_id)
    }

    pub fn upload_file(&self, file: &Path) -> Fallible<BlobId> {
        let data = fs::read_to_string(file).context(format!(
            "reading file: {}",
            file.to_str().expect("file is an invalid UTF-8")
        ))?;
        self.upload(data)
    }

    fn get_providers(&self) -> Fallible<Vec<PeerInfo>> {
        let url = format!("http://{}/peers", self.hub_ip);
        let res = query_deserialize(Method::GET, &url, ())?.expect("No content");
        Ok(res)
    }

    pub fn get_download_cmd(
        &self,
        blob_id: BlobId,
        filename: String,
        fmt: ResourceFormat,
    ) -> Command {
        Command::DownloadFile {
            uri: format!(
                "http://{}/sessions/{}/blobs/{}",
                self.hub_ip, self.session_id, blob_id
            ),
            file_path: filename,
            format: fmt,
        }
    }
}

impl Drop for HubSession {
    fn drop(&mut self) {
        self.destroy().expect("Destroying the hub session failed");
    }
}

type BlobId = u64;

#[derive(Debug, Fail)]
#[fail(
    display = "Deserialization failed: {}. Original string: {}",
    error, raw_json
)]
struct DeserializationError {
    #[fail(cause)]
    error: serde_json::Error,
    raw_json: DisplayBytes,
}

#[derive(Debug)]
struct DisplayBytes(Bytes);
impl std::fmt::Display for DisplayBytes {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        match str::from_utf8(&self.0) {
            Ok(s) => write!(fmt, "{}", s),
            Err(e) => write!(fmt, "data was not valid UTF-8: {}", e),
        }
    }
}

impl DeserializationError {
    fn new(error: serde_json::Error, raw_json: Bytes) -> Self {
        let raw_json = DisplayBytes(raw_json);
        Self { error, raw_json }
    }
}

// When using this function, the type should be explicitly annotated!
// This is probably a bug in the Rust compiler
// see https://github.com/rust-lang/rust/issues/55928
fn query_deserialize_json_fut<T, U>(
    method: actix_web::http::Method,
    url: &str,
    payload: T,
) -> impl Future<Item = Option<U>, Error = failure::Error>
where
    T: Serialize,
    for<'a> U: Deserialize<'a>,
    U: Sync + Send + 'static,
{
    use actix_web::http::StatusCode;

    ClientRequest::build()
        .method(method)
        .timeout(Duration::from_secs(60 * 60 * 24 * 366)) // 1 year of timeout
        .uri(url)
        .json(&payload)
        .into_future()
        .and_then(|req| req.send().from_err())
        .from_err()
        .and_then(|resp| {
            let status = resp.status();
            match status {
                StatusCode::NO_CONTENT => None,
                _ => Some(
                    resp.body()
                        .limit(1024 * 1024 * 1024) // maximum payload: 1 GiB
                        .from_err()
                        .and_then(|bytes| {
                            serde_json::from_slice(bytes.as_ref())
                                .map_err(|err| DeserializationError::new(err, bytes).into())
                        }),
                ),
            }
        })
}

fn query_deserialize<T, U>(
    method: actix_web::http::Method,
    url: &str,
    payload: T,
) -> Fallible<Option<U>>
where
    T: Serialize,
    for<'a> U: Deserialize<'a>,
    U: Sync + Send + 'static,
{
    let fut = query_deserialize_json_fut(method, url, payload);
    wait_ctrlc(fut)
}
