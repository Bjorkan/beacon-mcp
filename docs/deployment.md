# Python deployment and rollback

Run `uv sync --locked` with Python 3.13.14 and uv 0.11.29 for development.
The container installs production dependencies and the wheel into a virtual
environment. Runtime includes no uv, Cargo, compiler, Node, npm or application
test suite. Package JSON contracts are bundled in the wheel; no startup download
occurs. The image runs as UID/GID 1000 and never writes application data.

For production, **pull a published image**, using its immutable digest:

```sh
export BEACON_MCP_IMAGE=ghcr.io/bjorkan/beacon-mcp@sha256:<published-digest>
docker compose pull
docker compose up -d
```

Keep the existing environment variables. Compose applies a read-only rootfs,
capability drop, no-new-privileges, 256 MiB, 0.5 CPU and 100 PIDs. TLS and any
access policy belong at the reverse proxy. Do not buffer SSE; allow long-lived
POSTs and SDK keepalives. Listeners must stay on their replica, refetch resources
and re-listen after disconnects. There is no replay or persistent store.

Uvicorn uses its standard shutdown: it drains requests up to the configured
grace deadline, then SDK cancellation and the Server lifespan clean up Beacon
socket/debounce tasks and the HTTP client. Active SSE can end abruptly; clients
must refetch and re-listen. CLI signal status follows Uvicorn/OS defaults.
Shutdown grace is rounded up to whole seconds. Compose allows 15 seconds by
default; increase `stop_grace_period` when increasing `SHUTDOWN_GRACE_MS`.
Health/readiness reports process availability without probing Beacon.

## Release process

The Python SDK v2 migration is already merged into `main`. Pull requests
are checked by Ruff, BasedPyright strict, pytest, distribution build and the
container verification job. A successful push to `main` (or an approved
version tag) publishes a new container image to GHCR; publishing is not a
production deployment.

Update the production `BEACON_MCP_IMAGE` digest only after reviewing the
published image and the target environment. Keep a known-good image digest
available for rollback.

## Rollback

The pre-migration main commit is `d12b0ba94868e03ad049e14596172d765c056274`.
Its published image was resolved and verified before cutover:

```sh
export BEACON_MCP_IMAGE=ghcr.io/bjorkan/beacon-mcp@sha256:27c846678830eab4c86747fe4319116dd345f5b29bb1620a03b8babe55a9a7ce
docker compose pull
docker compose up -d
```

This is the previous Rust release image, not a build on the deployment host.
The gateway has no database, volume or state migration to reverse. Reconnect
clients after rollback. Any source rollback requires a separately reviewed
change; never rewrite main or merge only part of the migration stack.
