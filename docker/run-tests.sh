#!/bin/sh
# This script needs to use docker-compose exec -T, because docker-compose on
# Ubuntu 18.04 is too old and it suffers from this bug:
# https://github.com/docker/compose/issues/4290
#
# If the relevant golem-unlimited branch has changed since
# last build, it's needed to use the `--no-clean` option.

DOCKER_BUILD_ARGS=""
for var in "$@"; do
	case "$var" in
		"-h"|"--help")
			echo "Usage: $0 [--no-cache]"
			;;
		"--no-cache")
			DOCKER_BUILD_ARGS="--no-cache"
			;;
	esac
done

check_cmd() {
	if ! command -v "$1" >/dev/null; then
		echo "Error: program missing from PATH: $1. Exiting..."
		exit 1
	fi
}
check_cmd jq

cleanup() {
	docker-compose down
}
trap cleanup EXIT

set -x
set -e

PRIVKEY_PATH=prov/ssh/mpi
if [ ! -f $PRIVKEY_PATH ]; then
	mkdir -p "$(dirname $PRIVKEY_PATH)"
	ssh-keygen -f $PRIVKEY_PATH -N ""
fi

docker-compose build "$DOCKER_BUILD_ARGS"
docker-compose up -d

HUB_ADDR=$(docker-compose exec -T hub gu-hub --json lan list -I hub | grep -v INFO | jq -r '.[0].Addresses')

for idx in $(seq 1 4); do
	docker-compose exec -T --index="$idx" prov gu-provider hubs connect "$HUB_ADDR"
done

# The providers need a while to connect to the hub, give them that while.
sleep 2

docker-compose exec -T hub gu-hub peer list

docker-compose exec -T hub gumpi -h "$HUB_ADDR" -j /examples/game-life.toml -n 4

# Check the correctness of the output
docker-compose exec -T hub tar -xvf game-life-outs.tar
docker-compose exec -T hub cmp game-life-output.txt /examples/correct-output.txt

echo "TEST PASSED"
