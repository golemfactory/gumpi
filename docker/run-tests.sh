#!/bin/sh

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
docker-compose up -d

HUB_ADDR=$(docker-compose exec hub gu-hub --json lan list -I hub | grep -v INFO | jq -r '.[0].Addresses')

for idx in $(seq 1 4); do
	docker-compose exec --index="$idx" prov gu-provider hubs connect "$HUB_ADDR"
done

# The providers need a while to connect to the hub, give them that while.
sleep 2

docker-compose exec hub gu-hub peer list

docker-compose exec hub gumpi -h "$HUB_ADDR" -j /examples/game-life.toml -n 4

# Check the correctness of the output
docker-compose exec hub tar -xvf game-life-output.tar
docker-compose exec hub cmp game-life-output.txt /examples/correct-output.txt
