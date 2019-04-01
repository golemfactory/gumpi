#!/bin/sh

check_cmd() {
	if ! command -v "$1" > /dev/null
	then
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

for idx in $(seq 1 4)
do
	docker-compose exec --index="$idx" prov gu-provider hubs connect "$HUB_ADDR"
done

sleep 2

docker-compose exec hub gu-hub peer list

# The result of this task should be 242611
docker-compose exec hub gumpi -h "$HUB_ADDR" -j /examples/game-life.toml -n 4
