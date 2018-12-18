use failure::{Fallible, ResultExt};
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, Serialize, Deserialize)]
pub struct JobConfig {
    pub progname: String,
    pub args: Vec<String>,
    pub mpiargs: Option<Vec<String>>,
    pub sources: Option<PathBuf>,
}

impl JobConfig {
    pub fn from_file(path: &Path) -> Fallible<Self> {
        let mut file = File::open(path).context(format!(
            "Failed to open configuration file: {}",
            path.display()
        ))?;
        let mut cfgstr = String::new();
        file.read_to_string(&mut cfgstr)
            .context("Failed to read the configuration file")?;
        let config: Self = toml::from_str(&cfgstr).context("Failed to load configuration")?;
        Ok(config)
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "gumpi", about = "MPI on Golem Unlimited")]
pub struct Opt {
    #[structopt(short = "n", long = "numproc")]
    pub numproc: u32,
    #[structopt(short = "h", long = "hub")]
    pub hub: SocketAddr,
    #[structopt(short = "j", long = "job")]
    pub jobconfig: PathBuf,
}
