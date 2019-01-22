use failure::{format_err, Fallible, ResultExt};
use gu_model::envman::SessionUpdate;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::rc::Rc;

mod gu_struct;
pub mod mpi;

use self::gu_struct::*;
pub use gu_model::envman::Command;
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
        let payload = json!({
            "image": {
                "url": "http://52.31.143.91/images/gumpi-image.tar.gz",
                "hash": "44d65afc45b1a78c3976b6fe42f4dec6253923bb7c671862556f841034d256a0"
            },
            "name": "monero mining",
            "tags": [],
            "note": "None",
            "envType": "hd"
        });
        debug!("payload: {}", payload);

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

        let payload = json!({
            "session_id": self.session_id
        });

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
        let payload = json!(null);

        let hw = self.hub_session.post_provider(id, service, &payload)?;
        Ok(hw)
    }

    pub fn post_service<T, U>(&self, service: u32, json: &T) -> Fallible<U>
    where
        T: Serialize,
        for<'a> U: Deserialize<'a>,
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

    // TODO return type?
    pub fn download(&self, blob_id: BlobId, filename: String) -> Fallible<Vec<String>> {
        let cmd = self.hub_session.get_download_cmd(blob_id, filename);
        self.exec_commands(vec![cmd])
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
        let session_id: u64 = query_deserialize(Method::POST, &url, payload)?;
        let session = HubSession { session_id, hub_ip };
        Ok(session)
    }

    #[allow(clippy::let_unit_value)]
    fn destroy(&self) -> Fallible<()> {
        let url = format!("http://{}/sessions/{}", self.hub_ip, self.session_id);
        let _reply: () = query_deserialize(Method::DELETE, &url, json!({}))?;
        //info!("Reply: {}", reply);
        Ok(())
    }

    // When using this function, the type should be explicitly annotated!
    // This is probably a bug in the Rust compiler
    // see https://github.com/rust-lang/rust/issues/55928
    fn post_provider<T, U>(&self, provider: NodeId, service: u32, json: &T) -> Fallible<U>
    where
        T: Serialize,
        for<'a> U: Deserialize<'a>,
    {
        let url = format!(
            "http://{}/peers/send-to/{}/{}",
            self.hub_ip,
            provider.to_string(),
            service
        );
        let payload = json!({ "b": json });
        // The provider actually returns a Result<U, _>.
        // reply will be Err(_) if an error occurred on the provider
        // side, but the HTTP request succeeded.
        // Still, if there was an error on the provider side, we want to
        // propagate the error, hence the map_err.
        let reply: Result<U, String> = query_deserialize(Method::POST, &url, payload)?;
        reply.map_err(|err| format_err!("Provider replied: {}", err))
    }

    /// Returns: blob id
    pub fn upload(&self, payload: String) -> Fallible<BlobId> {
        info!("Creating a slot");
        let url = format!("http://{}/sessions/{}/blobs", self.hub_ip, self.session_id);
        let blob_id: u64 = query_deserialize(Method::POST, &url, json!({}))?;
        let url = format!("{}/{}", url, blob_id);
        info!("Uploading a file, id = {}", blob_id);

        let client = reqwest::Client::new();
        client.put(&url).body(payload).send()?;

        info!("Uploaded a file, id = {}", blob_id);
        Ok(blob_id)
    }

    pub fn upload_file(&self, file: &Path) -> Fallible<BlobId> {
        let data = fs::read_to_string(file).context("reading the file")?;
        self.upload(data)
    }

    fn get_providers(&self) -> Fallible<Vec<PeerInfo>> {
        let url = format!("http://{}/peers", self.hub_ip);
        let mut reply = reqwest::get(&url)?;
        let status = reply.status();
        if status.is_success() {
            let info = reply.json()?;
            Ok(info)
        } else {
            Err(format_err!("Hub returned an error: {}", status))
        }
    }

    pub fn get_download_cmd(&self, blob_id: BlobId, filename: String) -> Command {
        Command::DownloadFile {
            uri: format!(
                "http://{}/sessions/{}/blobs/{}",
                self.hub_ip, self.session_id, blob_id
            ),
            file_path: filename,
        }
    }
}

impl Drop for HubSession {
    fn drop(&mut self) {
        self.destroy().expect("Destroying the hub session failed");
    }
}

type BlobId = u64;

// This function is too general and hacky. Not everything in GU will return a valid JSON.
// See issue https://github.com/golemfactory/gumpi/issues/9
//
// When using this function, the type should be explicitly annotated!
// This is probably a bug in the Rust compiler
// see https://github.com/rust-lang/rust/issues/55928
fn query_deserialize<T, U>(method: reqwest::Method, url: &str, payload: T) -> Fallible<U>
where
    T: Serialize,
    for<'a> U: Deserialize<'a>,
{
    let client = reqwest::Client::builder()
        .timeout(None)
        .build()
        .expect("Building a client");
    debug!(
        "Payload:\n {}",
        serde_json::to_string_pretty(&payload)
            .unwrap_or_else(|_| "serialization failed".to_owned())
    );
    let mut resp = client.request(method, url).json(&payload).send()?;
    if !resp.status().is_success() {
        return Err(format_err!(
            "Querying URL {} returned an error: {}",
            url,
            resp.status()
        ));
    }
    let mut content = resp.text().unwrap();
    debug!("Got reply from {}: {:?}", url, content);
    if content == "" {
        // nasty hack, "" is not a valid JSON and GU sometimes returns no output
        content = serde_json::to_string(&()).unwrap();
    }
    let resp_content: U =
        serde_json::from_str(&content).context(format!("Bad JSON: {}", content))?;
    Ok(resp_content)
}
