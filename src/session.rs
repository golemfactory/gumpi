use actix::prelude::*;
use actix_web::{client::ClientRequest, http::Method, HttpMessage};
use failure::{format_err, Fallible, ResultExt};
use futures::future::Either;
use futures::prelude::*;
use gu_model::envman::SessionUpdate;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::convert::Into;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

mod gu_struct;
pub mod mpi;

use self::gu_struct::*;
pub use gu_model::envman::{Command, CreateSession, DestroySession, Image};
use gu_model::peers::PeerInfo;
use gu_net::NodeId;

#[derive(Debug)]
struct ProviderSession {
    session_id: String,
    peerinfo: PeerInfo,
    hub_session: Rc<HubSession>,
}

impl ProviderSession {
    pub fn new(hub_session: Rc<HubSession>, peerinfo: PeerInfo) -> Fallible<Self> {
        let node_id = peerinfo.node_id;
        let service = 37;

        let payload = CreateSession {
            env_type: "hd".to_owned(),
            image: Image {
                url: "http://52.31.143.91/images/gumpi-image.tar.gz".to_owned(),
                hash: "SHA1:61014e38bf1b5cb94f61444e64400163ecbbdb14".to_owned(),
            },
            name: "gumpi".to_owned(),
            tags: vec![],
            note: None,
            options: (),
        };

        let session_id: String = hub_session
            .post_provider(node_id, service, &payload)
            .context("POST request")?;
        info!("Session id: {}", session_id);

        Ok(Self {
            session_id,
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

    fn get_hardware(&self) -> Fallible<Hardware> {
        let id = self.peerinfo.node_id;
        let service = 19354;

        let hw = self.hub_session.post_provider(id, service, &())?;
        Ok(hw)
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

    pub fn download(&self, blob_id: BlobId, filename: String) -> Fallible<String> {
        let cmd = self.hub_session.get_download_cmd(blob_id, filename);
        let mut results = self.exec_commands(vec![cmd])?;
        assert_eq!(results.len(), 1);
        Ok(results.swap_remove(0))
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

    /// Returns: blob id
    pub fn upload(&self, payload: String) -> Fallible<BlobId> {
        info!("Creating a slot");
        let url = format!("http://{}/sessions/{}/blobs", self.hub_ip, self.session_id);
        let blob_id: u64 = query_deserialize(Method::POST, &url, ())?.expect("No content");
        let url = format!("{}/{}", url, blob_id);
        info!("Uploading a file, id = {}", blob_id);
        debug!("File contents: {}", payload);

        let future = ClientRequest::build()
            .method(Method::PUT)
            .timeout(Duration::from_secs(60 * 60 * 24 * 366)) // 1 year of timeout
            .uri(&url)
            .body(&payload)
            .into_future()
            .and_then(|req| req.send().from_err())
            .map_err(|e| e.into())
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
        let data = fs::read_to_string(file).context("reading the file")?;
        self.upload(data)
    }

    fn get_providers(&self) -> Fallible<Vec<PeerInfo>> {
        let url = format!("http://{}/peers", self.hub_ip);
        let res = query_deserialize(Method::GET, &url, ())?.expect("No content");
        Ok(res)
    }

    pub fn get_download_cmd(&self, blob_id: BlobId, filename: String) -> Command {
        Command::DownloadFile {
            uri: format!(
                "http://{}/sessions/{}/blobs/{}",
                self.hub_ip, self.session_id, blob_id
            ),
            file_path: filename,
            format: Default::default(),
        }
    }
}

impl Drop for HubSession {
    fn drop(&mut self) {
        self.destroy().expect("Destroying the hub session failed");
    }
}

type BlobId = u64;

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
        .and_then(|resp| {
            let status = resp.status();
            match status {
                // TODO print the raw json in the context
                StatusCode::NO_CONTENT => None,
                _ => Some(resp.json().from_err()),
            }
        })
        .map_err(|e| e.into())
}

fn wait_ctrlc<F>(future: F) -> Fallible<F::Item>
where
    F: Future<Error = failure::Error>,
{
    let mut sys = System::new("gumpi");
    let ctrlc = tokio_signal::ctrl_c()
        .flatten_stream()
        .into_future()
        .map_err(|_| ());
    let fut = future.select2(ctrlc);
    sys.block_on(fut)
        .map_err(|e| match e {
            Either::A((e, _)) => e,
            _ => panic!("Ctrl-C handling failed"),
        })
        .and_then(|res| match res {
            Either::A((r, _)) => Ok(r),
            Either::B(_) => {
                info!("Ctrl-C received, cleaning-up...");
                Err(format_err!("Ctrl-C received..."))
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
