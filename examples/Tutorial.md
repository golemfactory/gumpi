# How to run parallel tasks with gumpi
As an example, we're going to run a simple MPI application,
simulating the Conway's Game of Life.

Gumpi job configs are TOML files.
You can find a documented example config file: `examples/game-life.toml`.

First prepare the sources.

```
git clone https://github.com/marmistrz/game-life.git
cd game-life
git archive --format tar -o game-life.tar master
```

Put the resulting tarball into the folder where the job config
resides. In this case it's going to be the `examples` subdirectory.

`game-life` is expected to have the following command-line

```
Usage: ./game-life procs-row procs-col plane-dimension timesteps
```

hence, using purely MPI, we would use the following command to run the application using 12 processes using a 4x3 process grid, on a 12000x12000 grid and simulate 10 steps:

```
mpirun -n 12 ./game-life 4 3 12000 10
```

This corresponds to the first part of the config file:
```
progname = "game-life"
args = ["4", "3", "12000", "10"]
```

Now we execute the task using gumpi:
```
cargo run -- -h 127.0.0.1:61622 --job examples/game-life.toml -n 12
```
