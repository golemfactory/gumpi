#!/bin/bash
set -e

run_test() {
    cargo run -- -h 127.0.0.1:61622 -j "$1.toml" -n 2 -t 1
}

for test in "threads"; do
    echo "Running test '$test'"
    run_test $test
    echo "OK"
done

echo "All tests passed!"