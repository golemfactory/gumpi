use failure::{format_err, Fallible, ResultExt};
use serde::Deserialize;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};

mod gu_struct;
pub mod mpi;

use self::gu_struct::*;
use gu_net::NodeId;

struct Session {
    session_id: String,
    node_id: NodeId,
}

pub struct SessionMan {
    hub_ip: SocketAddr,
    session: Option<Session>,
}

#[derive(Debug)]
pub struct Provider {
    id: NodeId,
    ip: IpAddr,
    cpus: usize,
}

impl Drop for SessionMan {
    fn drop(&mut self) {
        self.destroy().expect("Destroying the session failed");
    }
}

impl SessionMan {
    pub fn new(hub_ip: SocketAddr) -> Self {
        SessionMan {
            hub_ip,
            session: None,
        }
    }

    fn post_provider<U>(
        &self,
        provider: &NodeId,
        service: u32,
        json: &serde_json::Value,
    ) -> Fallible<U>
    where
        for<'a> U: Deserialize<'a>,
    {
        let client = reqwest::Client::new();
        let url = format!(
            "http://{}/peer/send-to/{}/{}",
            self.hub_ip,
            provider.to_string(),
            service
        );
        let payload = json!({ "b": json });
        let mut resp = client.post(&url).json(&payload).send()?;
        let content = resp.text().unwrap();
        info!("Got reply: {}", content);
        let resp_content: Result<U, String> = serde_json::from_str(&content).context("Bad JSON")?;
        resp_content.map_err(|e| format_err!("Provider replied: {:?}", e))
    }

    pub fn create(&mut self, node: NodeId) -> Fallible<()> {
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
            .post_provider(&node, service, &payload)
            .context("POST request")?;
        info!("Session id: {}", session_id);
        self.session = Some(Session {
            session_id,
            node_id: node, // TODO find a way to use a reference without explicit lifetimeing
        });
        Ok(())
    }

    /// this method is private, destruction is a part of RAII
    /// if no session has been established, this is a no-op
    fn destroy(&mut self) -> Fallible<()> {
        let service = 40;

        if let Some(ref session) = self.session {
            let payload = json!({
                "session_id": session.session_id
            });

            let reply: String = self
                .post_provider(&session.node_id, service, &payload)
                .context("POST request")?;
            info!("Reply: {}", reply);
            self.session = None;
        }

        Ok(())
    }

    /*pub fn exec(&self, executable: &str, args: &[&str]) -> Fallible<()> {
        let id = 38;
        let payload = json!({
            "sessionId": self.session_id,
            "commands": [{
                "exec": {
                    "executable": executable,
                    "args": args
                }
            }]
        });
        info!("Payload is:\n{}", payload);
        let reply: Vec<String> = self.post_provider(id, payload).context("POST request")?;
        println!("Output:\n{:?}", reply);
        Ok(())
    }*/

    fn get_providers(&self) -> Fallible<Vec<PeerInfo>> {
        let url = format!("http://{}/peer", self.hub_ip);
        let mut reply = reqwest::get(&url)?;
        let info = reply.json()?;
        Ok(info)
    }

    pub fn get_provider_info(&self) -> Fallible<Vec<Provider>> {
        let providers = self.get_providers()?;
        let res = providers
            .into_iter()
            .map(|info| -> Fallible<Provider> {
                let id = info.node_id;
                let service = 19354;
                let payload = json!(null);

                let hw = self.post_provider(&id, service, &payload);
                let hw: Hardware = hw.context(format!("POST to {}", id.to_string()))?;

                // TODO handle peers without an IP
                let ip_port: SocketAddr = info.peer_addr.unwrap().parse().unwrap();

                Ok(Provider {
                    id,
                    ip: ip_port.ip(),
                    cpus: hw.num_cores,
                })
            })
            .filter_map(Result::ok)
            .collect();
        Ok(res)
    }

    /*pub fn provider_hwinfo(&self, provider_addr: SocketAddr) {
        let id = 19354;
        let payload = json!({
            "b": null
        });
        let reply: Hardware = self.post_provider(id: u32, json: T)
    }*/

    /*pub fn new_session(ip: String) -> Self {
        let mgr = Self::new(ip);
        mgr.conne
    }*/
}
