extern crate gu_envman_api;

use crate::failure_ext::OptionExt;
use failure::{format_err, Fallible, ResultExt};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::{IpAddr, SocketAddr};

mod gu_struct;
pub mod mpi;

use self::gu_struct::*;
pub use gu_envman_api::Command;
use gu_envman_api::SessionUpdate;
use gu_net::NodeId;

struct ProviderSession {
    session_id: String,
    node_id: NodeId,
}

struct HubSession {
    session_id: u64,
}

pub struct SessionMan {
    hub_ip: SocketAddr,
    provider_session: Option<ProviderSession>,
    hub_session: HubSession,
}

#[derive(Debug)]
pub struct Provider {
    id: NodeId,
    ip: IpAddr,
    cpus: usize,
}

impl Drop for SessionMan {
    fn drop(&mut self) {
        self.destroy_provider_session()
            .expect("Destroying the provider session failed");
        self.destroy_hub_session()
            .expect("Destroying the hub session failed");
    }
}

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
        // nasty hack
        content = serde_json::to_string(&()).unwrap();
    }
    let resp_content: U =
        serde_json::from_str(&content).context(format!("Bad JSON: {}", content))?;
    Ok(resp_content)
}

impl SessionMan {
    pub fn new(hub_ip: SocketAddr) -> Fallible<Self> {
        Ok(SessionMan {
            hub_ip,
            provider_session: None,
            hub_session: Self::init_hub_session(hub_ip)?,
        })
    }

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
    pub fn upload(&self, payload: String) -> Fallible<u64> {
        info!("Creating a slot");
        let session = &self.hub_session;
        let url = format!(
            "http://{}/sessions/{}/blobs",
            self.hub_ip, session.session_id
        );
        let blob_id: u64 = query_deserialize(Method::POST, &url, json!({}))?;
        let url = format!("{}/{}", url, blob_id);
        info!("Uploading a file, id = {}", blob_id);

        let client = reqwest::Client::new();
        client.put(&url).body(payload).send()?;

        info!("Uploaded a file, id = {}", blob_id);
        Ok(blob_id)
    }

    fn init_hub_session(hub_ip: SocketAddr) -> Fallible<HubSession> {
        info!("Initializing a hub session");
        let url = format!("http://{}/sessions", hub_ip);
        let payload = SessionInfo {
            name: "gumpi".to_owned(),
            environment: "hd".to_owned(),
        };
        let session_id: u64 = query_deserialize(Method::POST, &url, payload)?;
        let session = HubSession { session_id };
        Ok(session)
    }

    #[allow(clippy::let_unit_value)]
    fn destroy_hub_session(&self) -> Fallible<()> {
        let url = format!(
            "http://{}/sessions/{}",
            self.hub_ip, self.hub_session.session_id
        );
        let _reply: () = query_deserialize(Method::DELETE, &url, json!({}))?;
        //info!("Reply: {}", reply);
        Ok(())
    }

    pub fn init_provider_session(&mut self, node: NodeId) -> Fallible<()> {
        let service = 37;
        let payload = json!({
            "image": {
                "url": "http://52.31.143.91/images/monero-linux.tar.gz",
                "hash": "45b4aad70175ebefdbd1ca26d549fbee74a49d51"
            },
            "name": "monero mining",
            "tags": [],
            "note": "None",
            "envType": "hd"
        });

        let session_id: String = self
            .post_provider(node, service, &payload)
            .context("POST request")?;
        info!("Session id: {}", session_id);
        self.provider_session = Some(ProviderSession {
            session_id,
            node_id: node, // TODO find a way to use a reference without explicit lifetimeing
        });
        Ok(())
    }

    /// this method is private, destruction is a part of RAII
    /// if no session has been established, this is a no-op
    fn destroy_provider_session(&mut self) -> Fallible<()> {
        let service = 40;

        if let Some(ref session) = self.provider_session {
            let payload = json!({
                "session_id": session.session_id
            });

            let reply: String = self
                .post_provider(session.node_id, service, &payload)
                .context("POST request")?;
            info!("Reply: {}", reply);
            self.provider_session = None;
        }

        Ok(())
    }

    fn get_provider_session(&self) -> Fallible<&ProviderSession> {
        let session = self
            .provider_session
            .as_ref()
            .ok_or_context("Provider not initialized")?;
        Ok(session)
    }

    pub fn exec_commands(&self, cmd: Vec<Command>) -> Fallible<Vec<String>> {
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

    pub fn get_provider_info(&self) -> Fallible<Vec<Provider>> {
        let providers = self.get_providers()?;
        let res = providers
            .into_iter()
            .filter_map(|info| {
                let id = info.node_id;
                let service = 19354;
                let payload = json!(null);

                let hw = self.post_provider(id, service, &payload);
                let hw: Hardware = match hw {
                    Ok(hw) => hw,
                    Err(e) => {
                        error!("Error while querying provider info: {}", e);
                        return None;
                    }
                };

                // TODO handle peers without an IP
                let ip_port: SocketAddr = info.peer_addr.unwrap().parse().unwrap();

                Some(Provider {
                    id,
                    ip: ip_port.ip(),
                    cpus: hw.num_cores,
                })
            })
            .collect();
        Ok(res)
    }
}
