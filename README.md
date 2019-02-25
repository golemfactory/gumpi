# gumpi
CircleCI status: [![status](https://circleci.com/gh/golemfactory/gumpi.svg?style=svg)](https://circleci.com/gh/golemfactory/gumpi)

Known to work with [this GU version](https://github.com/golemfactory/golem-unlimited/commit/6f910289c03d916ad3d3683be2f05454f099082a).

Minimum supported version:
* Rust: 1.32
* OpenMPI: 3.0

# Docker image

gumpi requires at least OpenMPI 3.0 on the provider machine. Since the current LTS version of Ubuntu only has OpenMPI 2.x, you can find a compatible Docker image [here](https://github.com/marmistrz/docker-openmpi).

# Example usage

If the hub listens at `127.0.0.1:61622` and you want to spawn `12` processes, enter `gumpi` project root, execute:
```
cargo run -- --hub 127.0.0.1:61622 --job job.toml -n 12
```

See the command line help for more information about the parameters

Example job config (`job.toml` from the example) which will show the hostname for every process on every node:

```
progname = "uname"
args = ["-a"]
```

# Known issues
If you want to run the application over LAN, you may need to specify your IP address space, e.g. the previous example will become:

```
progname = "uname"
args = ["-a"]
mpiargs = ["--mca", "btl_tcp_if_include", "10.30.8.0/22"]
```
