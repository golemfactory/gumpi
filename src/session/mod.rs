use failure::{format_err, Fallible, ResultExt};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub mod mpi;

pub struct SessionMan {
    provider_ip: String,
    hub_ip: String,
    session_id: Option<String>,
}

// TODO use submodule from gu-net
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub node_name: String,
    pub peer_addr: Option<String>,
    // pub node_id: NodeId,
    // pub sessions: Vec<PeerSessionInfo>,
    // pub tags: Vec<String>,
}

impl Drop for SessionMan {
    fn drop(&mut self) {
        self.destroy().expect("Destroying the session failed");
    }
}

impl SessionMan {
    pub fn new(root_provider_ip: String, hub_ip: String) -> Self {
        SessionMan {
            provider_ip: root_provider_ip,
            hub_ip,
            session_id: None,
        }
    }

    fn post_provider<T, U>(&self, id: u32, json: T) -> Fallible<U>
    where
        T: Serialize,
        for<'a> U: Deserialize<'a>,
    {
        let client = reqwest::Client::new();
        let url = format!("http://{}/m/{}", self.provider_ip, id);
        let mut resp = client.post(&url).json(&json).send()?;
        let content = resp.text().unwrap();
        info!("Got reply: {}", content);
        let resp_content: Result<U, String> = serde_json::from_str(&content).context("Bad JSON")?;
        resp_content.map_err(|e| format_err!("Provider replied: {:?}", e))
    }

    pub fn create(&mut self) -> Fallible<()> {
        let id = 37;
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

        let session_id: String = self.post_provider(id, payload).context("POST request")?;
        info!("Session id: {}", session_id);
        self.session_id = Some(session_id);
        Ok(())
    }

    pub fn destroy(&mut self) -> Fallible<()> {
        let id = 40;
        let payload = json!({
            "session_id": self.session_id
        });

        let reply: String = self.post_provider(id, payload).context("POST request")?;
        info!("Reply: {}", reply);
        self.session_id = None;
        Ok(())
    }

    pub fn exec(&self, executable: &str, args: &[&str]) -> Fallible<()> {
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
    }

    pub fn get_providers(&self) -> Fallible<Vec<PeerInfo>> {
        let url = format!("http://{}/peer", self.hub_ip);
        let mut reply = reqwest::get(&url)?;
        let info = reply.json()?;
        Ok(info)
    }

    /*pub fn new_session(ip: String) -> Self {
        let mgr = Self::new(ip);
        mgr.conne
    }*/
}
