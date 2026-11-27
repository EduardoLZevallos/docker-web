# Docker Web

A read-only Docker network topology viewer for a single Linux host. The
backend queries the local Docker daemon through a private, deny-by-default
Unix-socket proxy. The production viewer is a static Vite build served by
nginx on loopback, with `/api/*` proxied to the Rust backend on the same origin.

## Production quickstart

Prerequisites:

- Linux with Docker Engine and Docker Compose v2.
- Access to the rootful Docker socket at `/var/run/docker.sock`, normally by
  running Docker as root or as a user in the `docker` group.
- Outbound registry access on the first build for the Rust, Node, nginx, and
  Debian base images.
- Port `8080` free on `127.0.0.1`. Port `5173` is reserved for the development
  and demo Vite server and is not used by this deployment.
- `backend/Cargo.lock` present in the checkout. The backend image builds with
  `cargo build --locked` and will reject a missing or stale lockfile.

Start the stack:

```sh
docker compose up --build
```

Or run it detached:

```sh
docker compose up --build -d
docker compose ps
```

Open <http://127.0.0.1:8080> and verify the loopback health endpoint:

```sh
curl -fsS http://127.0.0.1:8080/api/health
```

A healthy response includes `"status":"healthy"` and `"docker":true`. The
compose healthchecks also cover the socket proxy, backend, and web container.

Stop the stack and remove its compose-owned network and private socket volume:

```sh
docker compose down -v --remove-orphans
```

The default compose project is `docker-web`. For a disposable test stack, use a
unique project name so it cannot overlap the backend E2E or demo fixtures:

```sh
docker compose -p docker-web-prod-smoke-$$ up --build -d
docker compose -p docker-web-prod-smoke-$$ down -v --remove-orphans
```

## Services

| Service | Purpose | Exposure |
| --- | --- | --- |
| `docker-socket-proxy` | Private allowlisted Docker API proxy over a Unix socket | No ports; reads the host socket read-only |
| `backend` | Rust API using `/run/docker-api/docker.sock` | No host ports; internal port `3000` |
| `web` | Static Vite build plus same-origin `/api/*` proxy | `127.0.0.1:8080` only |

The real host Docker socket is mounted only into `docker-socket-proxy`, and
read-only. The backend mounts only the shared named volume
`docker-api-socket` read-only. The web container mounts neither the host socket
nor the private socket volume. There is no Docker TCP listener.

The proxy creates the private socket directory as `root:dockerproxy` mode
`0770` and the socket as mode `0660`; the backend runs as UID/GID `10001`.
Only the proxy and backend can reach that socket.

## Docker API policy

The proxy is deny-by-default and accepts only `GET` requests for the client
operations the backend uses. Bollard 0.16 with API version `1.44` emits
unversioned Unix-socket paths, so the proxy accepts the actual request paths
and rewrites them to the versioned Docker API before forwarding:

- `GET /_ping` or `GET /v1.44/_ping`
- `GET /containers/json` or `GET /v1.44/containers/json` (query arguments are
  preserved; the backend sends `all=true`)
- `GET /networks` or `GET /v1.44/networks`

Every other path is forbidden, and every non-`GET` method is rejected. There
are no container create/start/stop/delete routes, no image, exec, volume, or
system routes, and no Docker TCP endpoint.

## Configuration

The compose stack mounts [`deploy/config/config.yaml`](deploy/config/config.yaml)
into the backend at `/app/config/config.yaml`. The production defaults use the
private socket, bind the backend only inside the compose network, poll every 30
seconds, and disable demo mode.

After editing the config, recreate the backend:

```sh
docker compose up -d --force-recreate backend
```

Use `DOCKER_HOST_SOCKET_PATH` to point the proxy at a non-default host socket
path, for example:

```sh
DOCKER_HOST_SOCKET_PATH=/run/user/1000/docker.sock docker compose up --build
```

The default supported deployment is a rootful Docker daemon. A rootless socket
may require additional ownership or capability adjustments to the proxy
container.

## Verification

Inspect the resolved compose model and check the loopback bind:

```sh
docker compose config
curl -fsS http://127.0.0.1:8080/api/config
curl -fsS http://127.0.0.1:8080/api/topology
```

The only published listener should be loopback:

```sh
ss -ltnp | grep ':8080'
```

Test the proxy boundary through the backend container without mutating Docker:

```sh
docker compose exec -T backend curl -sS --unix-socket /run/docker-api/docker.sock \
  -o /dev/null -w '%{http_code}\n' http://localhost/v1.44/_ping
docker compose exec -T backend curl -sS --unix-socket /run/docker-api/docker.sock \
  -o /dev/null -w '%{http_code}\n' http://localhost/v1.44/images/json
```

The first request should return `200`; the out-of-scope read should return
`403`.

## Latest production smoke

Validated on 2026-11-27 from a fresh source-only snapshot (no `.env` files,
`node_modules`, `dist`, `target`, logs, or test artifacts; includes
`backend/Cargo.lock`, Dockerfiles, `deploy/`, and proposed frontend files) using
a disposable compose project and the real host Docker daemon. `docker compose
config -q` and the image build passed. The loopback stack served `/` and
`/api/{health,config,topology}` on `127.0.0.1:8080`, reported
`"status":"healthy","docker":true`, and the topology container/network ID sets
matched the live daemon inventory. The production bundle contained no E2E
bridge. The socket proxy returned `200` for the six allowlisted GETs, `403` for
out-of-scope reads, and `405` for non-GET requests. Stopping only
`docker-socket-proxy` produced `"status":"degraded","docker":false` and a `503`
from `/api/topology`; starting it restored both on the same backend container
(same ID and start time). Project containers, network, and volume were removed
with `down -v --remove-orphans`; no prune was run.

Artifacts: `/tmp/opencode/mvp_prod_smoke_20261127T223101Z_3205067/` (snapshot,
manifest, checksums, and logs). The built `docker-web/*:local` images were left
in place, not pruned.

## Known single-host limitations

- One local Docker daemon per deployment. Remote Docker hosts, multiple
  contexts, Swarm managers, and Docker TCP endpoints are not supported.
- No authentication, authorization, TLS, or multi-user isolation. The viewer
  is intentionally published only on `127.0.0.1`. Do not expose it beyond
  loopback without adding an authenticated reverse proxy.
- The viewer reads the whole local daemon inventory, not a filtered per-user
  or per-project subset.
- Discovery is polling only; there is no Docker event stream or historical
  timeline.
- The proxy is an allowlist for the current read-only API needs, not a general
  Docker socket proxy. New backend Docker API calls require an explicit proxy
  and documentation change.
- Builds need `backend/Cargo.lock` and registry access. Runtime does not need
  outbound network access beyond the local Docker socket path.

## Development

- Backend E2E and demo fixture: [`backend/tests/e2e_assets/README.md`](backend/tests/e2e_assets/README.md)
- Frontend development and browser E2E: [`frontend/docker-web-ui/README.md`](frontend/docker-web-ui/README.md)

The development and demo paths use Vite on port `5173` and are separate from
the production compose stack.
