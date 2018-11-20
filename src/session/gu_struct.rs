// structs copied over from gu-net, gu-hardware
// TODO use submodule and gu-envman-api
use gu_net::NodeId;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub node_name: String,
    pub peer_addr: Option<String>,
    pub node_id: NodeId,
    // pub sessions: Vec<PeerSessionInfo>,
    // pub tags: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Hardware {
    /*#[serde(skip_serializing_if = "Option::is_none")]
    gpu: Option<GpuCount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ram: Option<RamInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disk: Option<DiskInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os: Option<OsType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,*/
    num_cores: usize,
}
