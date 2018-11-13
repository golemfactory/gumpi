use failure::{format_err, Fallible, ResultExt};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct SessionMan {
    provider_ip: String,
    session_id: Option<String>,
}

impl SessionMan {
    pub fn new(ip: String) -> Self {
        SessionMan {
            provider_ip: ip,
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
        let resp_content: Result<U, String> = resp.json().context("Bad JSON")?;
        resp_content.map_err(|e| format_err!("Provider replied: {:?}", e))
    }

    pub fn create(&mut self) -> Fallible<()> {
        let id = 37;
        let payload = json!({
            "image": {
                "cache_file": "xmr-stak.tgz",
                "url": "http://52.31.143.91/images/monero-linux.tar.gz"
            },
            "name": "monero mining",
            "tags": [],
            "note": "None"
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
            "session_id": self.session_id,
            "commands": [{
                "Exec": {
                    "executable": executable,
                    "args": args
                }
            }]
        });
        debug!("Payload is:\n{}", payload);
        let reply: Vec<String> = self.post_provider(id, payload).context("POST request")?;
        println!("Output:\n{:?}", reply);
        Ok(())
    }

    /*pub fn new_session(ip: String) -> Self {
        let mgr = Self::new(ip);
        mgr.conne
    }*/
}
