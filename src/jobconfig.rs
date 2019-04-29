use failure::{Fallible, ResultExt};
use gu_client::NodeId;
use serde_derive::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum BuildType {
    Make,
    CMake,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sources {
    pub path: PathBuf,
    pub mode: BuildType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputConfig {
    pub source: PathBuf,
    pub target: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobConfig {
    pub progname: String,
    pub args: Vec<String>,
    pub mpiargs: Option<Vec<String>>,
    pub sources: Option<Sources>,
    pub output: Option<OutputConfig>,
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
    pub numproc: usize,
    #[structopt(short = "h", long = "hub")]
    pub hub: SocketAddr,
    #[structopt(short = "j", long = "job")]
    pub jobconfig: PathBuf,
    #[structopt(
        long = "providers",
        help = "explictly select which providers to use, by their node id"
    )]
    pub providers: Vec<NodeId>,
    #[structopt(long = "noclean")]
    pub noclean: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_jobconfig() {
        let config: JobConfig = toml::from_str(
            r#"
            progname = "prog"
            args = ["4", "3"]
            mpiargs = ["--mca", "btl_tcp_if_include", "10.30.8.0/22"]

            [sources]
            path = "prog.zip"
            mode = "CMake"
            "#,
        )
        .unwrap();

        assert_eq!(config.progname, "prog");
        assert_eq!(config.args, vec!["4", "3"]);
        assert_eq!(
            config.mpiargs.unwrap(),
            vec!["--mca", "btl_tcp_if_include", "10.30.8.0/22"]
        );

        let sources = config.sources.unwrap();
        assert_eq!(sources.path, Path::new("prog.zip"));
        assert_eq!(sources.mode, BuildType::CMake);
    }
}
