#!/bin/bash
set -e

#credit: https://stackoverflow.com/a/28938235/3805131
RESET='\033[0m'    # Text Reset
RED='\033[1;31m'   # Bold Red
GREEN='\033[1;32m' # Bold Green

git clone https://github.com/marmistrz/game-life.git
(
    cd game-life
    git archive --format tar -o game-life.tar master
    mv game-life.tar ../examples
)

echo "Running the test..."
out=$(cargo run -- -h 127.0.0.1:61622 --job examples/game-life.toml -n 2)

# this is the outcome of the computation, check if it was ever printed
if [[ "$out" == *"242611"* ]]; then
    echo -e "${GREEN}OK${RESET}"
else
    echo "Output: "
    echo "$out"
    echo -e "${RED}FAIL!${RESET}"
    exit 1
fi
