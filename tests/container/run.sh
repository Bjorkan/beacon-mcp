#!/bin/sh
set -eu

image="${IMAGE_NAME:-beacon-mcp:container-test}"
network="beacon-mcp-test-$$"
gateway="beacon-mcp-gateway-$$"
mock="beacon-mcp-mock-$$"

cleanup() {
  docker rm -f "$gateway" "$mock" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker build --pull -t "$image" .
test "$(docker run --rm --entrypoint id "$image" -u)" != "0"
docker run --rm --entrypoint sh "$image" -c \
  "test ! -e /app/src && test ! -e /app/tests && ! command -v node && ! command -v npm && ! command -v cargo && ! command -v rustc && ! command -v uv && test -x /usr/local/bin/beacon-mcp"
docker network create "$network" >/dev/null
docker run -d --name "$mock" --network "$network" --no-healthcheck \
  -v "$PWD/tests/container/mock-beacon.py:/mock.py:ro" \
  python:3.13-slim@sha256:09a17cad02668ae61ece136b4fd36ac3849e14f670ebedb637ea3e15580525bc \
  python /mock.py >/dev/null
docker run -d --name "$gateway" --network "$network" -p 127.0.0.1::3000 \
  --read-only --tmpfs /tmp:rw,noexec,nosuid,size=16m --cap-drop=ALL \
  --security-opt=no-new-privileges:true --memory=256m --cpus=0.5 --pids-limit=100 \
  -e BEACON_BASE_URL=http://"$mock":8080 \
  "$image" >/dev/null

port="$(docker port "$gateway" 3000/tcp | sed 's/.*://')"
i=0
until PORT_TO_TEST="$port" python3 tests/container/smoke.py 2>/dev/null; do
  i=$((i + 1)); test "$i" -lt 30 || { docker logs "$gateway"; exit 1; }
  sleep 1
done
test "$(docker exec "$gateway" id -u)" = "1000"
test "$(docker inspect -f '{{.HostConfig.ReadonlyRootfs}}' "$gateway")" = "true"
test "$(docker inspect -f '{{.HostConfig.Memory}}' "$gateway")" = "268435456"
test "$(docker inspect -f '{{.HostConfig.NanoCpus}}' "$gateway")" = "500000000"
test "$(docker inspect -f '{{.HostConfig.PidsLimit}}' "$gateway")" = "100"
test "$(docker exec "$gateway" awk '/^CapEff:/ { print $2 }' /proc/1/status)" = "0000000000000000"
test "$(docker exec "$gateway" awk '/^NoNewPrivs:/ { print $2 }' /proc/1/status)" = "1"
if docker exec -u 0 "$gateway" touch /usr/local/bin/.write-test >/dev/null 2>&1; then
  echo "Read-only root filesystem check failed."
  exit 1
fi
docker exec "$gateway" /usr/local/bin/beacon-mcp --healthcheck
docker stop --timeout 15 "$gateway" >/dev/null
test "$(docker inspect -f '{{.State.ExitCode}}' "$gateway")" = "0"
echo "Python container integration checks passed."
