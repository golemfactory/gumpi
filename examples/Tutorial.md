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
cargo run -- -h 127.0.0.1:61622 --job examples/game-life.toml -n 12 -t 1
```
Here `-t 1` means that we want one thread per MPI process. For multi-level
parallel applications (such as MPI+OpenMP) you can use `-t N` to indicate
the number of threads you want to know.

Please note that this option **doesn't influence the actual number of
threads used by the app**,
but only affects the way the processes are distributed onto nodes. You need to
configure the actual number of threads used during runtime, e.g. using
`omp_set_num_threads`.
# Build system-specific notes
## CMake
Using CMake is failsafe when it comes to the location of the resulting binary -
CMake will make sure that it always ends up in the right place.

We build the Release CMake build flavor.

## Make
Unfortunately, this is not the case with generic Makefiles.
Make sure that your Makefile puts the resulting binary to the top-level project directory.

While the CMake build backend makes sure that the MPI wrappers (`mpicc`, `mpicxx`)are being used,
this is not the case for the Make backend. Please make sure that your Makefile uses these wrappers.

The main target will be built, just as though you executed `make` on your local machine.

# Compilation options

The binaries will be built separately on each node, so you may freely use the
`-march=native` compilation option to apply hardware-specific optimizations.
