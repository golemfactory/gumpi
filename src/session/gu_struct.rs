// structs copied over from gu-net, gu-hardware
use serde_derive::{Deserialize, Serialize};

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
    pub num_cores: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct SessionInfo {
    pub name: String,
    pub environment: String,
}
