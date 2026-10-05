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

docker build -t "$image" .
test "$(docker run --rm --entrypoint id "$image" -u)" != "0"
docker run --rm --entrypoint sh "$image" -c \
  "test ! -e /app/src && test ! -e /app/tests && node -e \"try { require.resolve('typescript'); process.exit(1) } catch {}\" && node -e \"try { require.resolve('vitest'); process.exit(1) } catch {}\""
docker network create "$network" >/dev/null
docker run -d --name "$mock" --network "$network" -v "$PWD/tests/container/mock-beacon.mjs:/mock.mjs:ro" node:24-bookworm-slim node /mock.mjs >/dev/null
docker run -d --name "$gateway" --network "$network" -p 127.0.0.1::3000 \
  --read-only --tmpfs /tmp:rw,noexec,nosuid,size=16m --cap-drop=ALL \
  --security-opt=no-new-privileges:true --memory=256m --cpus=0.5 --pids-limit=100 \
  -e BEACON_BASE_URL=http://"$mock":8080 \
  "$image" >/dev/null

port="$(docker port "$gateway" 3000/tcp | sed 's/.*://')"
i=0
until PORT_TO_TEST="$port" node -e "fetch('http://127.0.0.1:'+process.env.PORT_TO_TEST+'/healthz').then(r=>{if(!r.ok)process.exit(1)})" 2>/dev/null; do
  i=$((i + 1)); test "$i" -lt 30 || { docker logs "$gateway"; exit 1; }
  sleep 1
done
test "$(docker exec "$gateway" id -u)" != "0"
PORT_TO_TEST="$port" node -e "Promise.all(['/healthz','/readyz'].map(p=>fetch('http://127.0.0.1:'+process.env.PORT_TO_TEST+p).then(r=>{if(!r.ok)throw Error(p)})))"
PORT_TO_TEST="$port" node -e "fetch('http://127.0.0.1:'+process.env.PORT_TO_TEST+'/',{method:'POST',headers:{'content-type':'application/json',accept:'application/json, text/event-stream','mcp-protocol-version':'2026-07-28','mcp-method':'tools/call','mcp-name':'beacon_search_nodes'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/call',params:{name:'beacon_search_nodes',arguments:{limit:20},_meta:{'io.modelcontextprotocol/protocolVersion':'2026-07-28','io.modelcontextprotocol/clientInfo':{name:'container-test',version:'1.0.0'},'io.modelcontextprotocol/clientCapabilities':{}}}})}).then(r=>r.text()).then(t=>{if(!t.includes('container-node'))throw Error(t)})"
docker stop --timeout 5 "$gateway" >/dev/null
test "$(docker inspect -f '{{.State.ExitCode}}' "$gateway")" = "0"
echo "Container integration checks passed."
