# gumpi
CircleCI status: [![status](https://circleci.com/gh/golemfactory/gumpi.svg?style=svg)](https://circleci.com/gh/golemfactory/gumpi)

Known to work with [this GU version](https://github.com/golemfactory/golem-unlimited/tree/gumpi-freeze), commit 93c9f37e1765ad743a6b16209561e6374fb88e84.

Minimum supported version:
* Rust: 1.33
* OpenMPI: 3.0

# Docker image

gumpi requires at least OpenMPI 3.0 on the provider machine. Since the current LTS version of Ubuntu only has OpenMPI 2.x, you can find a compatible Docker image [here](https://github.com/marmistrz/docker-openmpi).

# Example usage

If the hub listens at `127.0.0.1:61622` and you want to spawn `12` processes, enter `gumpi` project root, execute:
```
cargo run -- --hub 127.0.0.1:61622 --job job.toml -n 12 -t 1
```

See the command line help for more information about the parameters

Example job config (`job.toml` from the example) which will show the hostname for every process on every node:

```
progname = "uname"
args = ["-a"]
```

See [examples/Tutorial.md](examples/Tutorial.md) for a more details.

# Known issues and limitations
## Connectivity
If you want to run the application over LAN, you may need to specify your IP address space, e.g.
the previous example will become:

```
progname = "uname"
args = ["-a"]
mpiargs = ["--mca", "btl_tcp_if_include", "10.30.8.0/22"]
```

## Output size
Currently gumpi limits the accepted size of the stdout and the returned output to 1GiB.
Should this be a problem for your application, please report an issue.

## Output location
Currently gumpi requires the application to put all the relevant artifacts into a single subdirectory.
In particular, fetching files directly from the working directory is NOT supported.

# Debugging
You can use the `--noclean` runtime option to disable the automatic cleanup of the sessions on the client side.
Note that in the future Golem Unlimited may automatically remove stale sessions on the provider side.
