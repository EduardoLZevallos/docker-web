# Docker Network Discovery UI

React, TypeScript, and Cytoscape frontend for the Docker network discovery backend. The application is built with Vite and reads only the live backend APIs.

## Requirements

- Node.js LTS (`^20.19.0 || >=22.12.0`; validated locally with Node 26.13.1)
- A running backend exposing `/api/config` and `/api/topology`

## Install

```sh
npm ci
```

## Development

The Vite server listens on `http://localhost:5173`. Requests to the same-origin `/api/*` path are proxied to the backend origin in the `BACKEND_URL` server environment variable. Do not append `/api` to `BACKEND_URL`.

```sh
BACKEND_URL=http://127.0.0.1:8080 npm run dev
```

`BACKEND_URL` defaults to `http://127.0.0.1:8080` when neither the server environment nor the frontend `.env` file provides it. The value is server-side proxy configuration and is not exposed to the browser.

## Build and preview

```sh
npm run build
BACKEND_URL=http://127.0.0.1:8080 npm run preview
```

Both `dev` and `preview` use the same dynamic `/api` proxy and listen on port `5173`.

## One-command local demo

Start the ignored backend demo fixture, wait for its structured readiness file,
and serve the real UI against that exact backend origin:

```sh
frontend/docker-web-ui/tests/run_demo.sh
```

The launcher starts the existing interactive demo through
`backend/tests/run_backend_e2e.sh`, prints `http://localhost:5173`, and holds
for `DEMO_HOLD_SECONDS` (default `600`). When the hold expires, or on Ctrl+C,
it stops Vite and waits for the backend harness to finish all Docker cleanup.
The original interactive command remains available:

```sh
DEMO_HOLD_SECONDS=600 backend/tests/run_backend_e2e.sh demo --ignored --nocapture
```

Launcher overrides: `DEMO_HOLD_SECONDS`, `DEMO_STARTUP_TIMEOUT_SECONDS`
(default `300`), `DEMO_VITE_STARTUP_TIMEOUT_SECONDS` (default `90`), and
`DEMO_SHUTDOWN_TIMEOUT_SECONDS` (default `180`).

## Browser E2E

The browser suite builds this UI with `VITE_E2E=true`, serves the production
build with Vite preview on port `5173`, and runs Playwright against the actual
Rust API backed by the Testcontainers-managed private DinD daemon. It uses no
API mocks or request interception.

Prerequisites:

- Linux Docker with privileged containers and bind mounts; Rust 1.90 and Cargo.
- Node 26.13.1 on `PATH`, for example by adding `~/.local/bin` or
  `~/.local/share/node/node-v26.13.1-linux-x64/bin`.
- Install dependencies and the Chromium browser once; the tests never install
  anything implicitly:
  ```sh
  cd frontend/docker-web-ui
  npm ci
  npx playwright install chromium
  ```

Run the complete browser path from the repository root (the launcher resolves
the package directory, so it also works from any other current directory):

```sh
frontend/docker-web-ui/tests/run_frontend_e2e.sh
```

The runner typechecks and builds the E2E bundle once with `npm --prefix`, sets
`E2E_UI_PREBUILT=true`, then invokes
`backend/tests/run_backend_e2e.sh playwright_frontend_e2e --ignored --exact
--nocapture`. Playwright previews that existing build instead of rebuilding it.
Running Playwright directly without the launcher still builds before preview.
Extra runner arguments are passed to Playwright exactly; the `PLAYWRIGHT_ARGS`
environment variable accepts whitespace-separated arguments.
Suite environment includes `E2E_FRONTEND_THEME=light|dark` (default `light`)
and deadline/shutdown overrides `FRONTEND_E2E_TIMEOUT_SECONDS` (default `3600`),
`FRONTEND_E2E_BUILD_TIMEOUT_SECONDS`, and
`FRONTEND_E2E_SHUTDOWN_TIMEOUT_SECONDS`. Playwright traces and screenshots are
retained on failure; `PLAYWRIGHT_OUTPUT_DIR` changes the artifact directory.
The backend provides `BACKEND_URL`, `E2E_DOCKER_SOCKET`, `E2E_RUN_ID`,
`E2E_DIND_ID`, `E2E_BACKEND_ID`, `E2E_RUN_LABEL`, and, when configured,
`E2E_OUTER_DOCKER_HOST` to the browser process.

Playwright owns its inner Docker fixtures. Every network and container is
created with `docker --host <E2E_DOCKER_SOCKET>` and carries both the harness
run label (`E2E_RUN_LABEL=E2E_RUN_ID`) and a per-test
`com.docker-web.e2e.case` label. Teardown removes only tracked IDs, sweeps the
unique per-test label, and verifies nothing remains. Expected node and edge sets
come from Docker inspect and `docker ps` status output, never from the API. The
outage scenario pauses only the exact `E2E_DIND_ID` through the outer Docker
endpoint, restores it in a `finally` block, and verifies `E2E_BACKEND_ID` kept
the same start time. The Playwright preview records its process group; the
harness stops that exact group after normal completion, failure, timeout, or
SIGINT/SIGTERM so an interrupted run leaves no preview server behind.

The 22 browser tests cover:

- 2, 3, and 4 container networks with exactly nine inspected edges.
- Unattached networks, multi-homed containers, created/exited containers, and
  paused containers with edge eligibility taken from Docker state.
- Canvas clicks and Cytoscape highlight classes, container details with real
  ports/IP/MAC values, network details, and selection clearing.
- Nullable metadata: a created container with no ports or addresses, and a real
  canvas click on the built-in `none` network with a null subnet.
- Case-insensitive name search, raw Docker status search, filtering of inactive
  containers, filtered counts, and the no-results state.
- Network-membership filtering by stable full Docker network ID, including an
  empty selected network that remains visible and selectable.
- Published-binding filtering for running and paused containers, combined with
  network membership and text search without inventing edges.
- Real dual-stack Docker metadata: separate IPv4/IPv6 attachment addresses,
  every IPAM CIDR, IPv4 `127/8`, wildcard, and IPv6 `::1` host binding classes,
  and bracket formatting for IPv6 host bindings.
- Inactive containers keeping retained attachment metadata while reporting zero
  current host bindings and no current graph membership edge.
- Manual refresh after real create/stop/remove mutations and automatic polling.
- A paused-DinD outage that keeps stale topology visible and recovers on the
  same backend and containers.
- An initial DinD outage where the UI shows no topology or fake fallback data
  and then recovers the real fixtures already present in Docker.
- A paused backend during a pending refresh: the refresh control stays disabled,
  repeated clicks do not send duplicate API requests, and the pending cycle
  completes after the backend resumes.
- A backend outage lasting beyond the 10-second request timeout that retains
  the stale topology with real configuration and topology errors, then recovers
  through automatic polling after the same backend resumes.
- Theme persistence across reload, keyboard list selection, keyboard fit, and a
  narrow responsive layout with no horizontal overflow.

## Last validated result

Validated on 2026-11-27 on branch `feature/frontend-backend-e2e` from the
repository root with `frontend/docker-web-ui/tests/run_frontend_e2e.sh`:
**22 passed (1.4m)**, exit 0, run ID `2997771-18ebcd6c53c5cb22`. The full log is
at `/tmp/opencode/mvp_block3_full_browser_e2e_final2.log` (not committed). The
run built the UI once and reused it for preview, used the real Rust backend and
private DinD daemon, and used Chromium with no API mocks or request interception.
The same fixture also passed the full backend contract target: **39 passed; 0
failed; 2 ignored** in 209.42 s.

## Scripts

- `npm run dev` - start the Vite development server on port 5173
- `npm run typecheck` - run TypeScript without emitting files
- `npm run build` - typecheck and create the production build in `dist/`
- `npm run build:e2e` - typecheck and build with `VITE_E2E=true`
- `npm run preview` - serve the production build on port 5173 with the `/api` proxy
- `npm run test:e2e` - run Playwright for an already prepared backend/UI environment

There is no unit-test script and no CRA test scaffold.

## Runtime behavior

- `/api/config` supplies `theme`, `demo_mode`, and, when available, `discovery_interval`.
- `/api/topology` is the only topology source. `demo_mode` never causes mock or fallback data.
- Requests time out after 10 seconds and are aborted when the application unmounts.
- The configured discovery interval is used for automatic refresh. A missing or invalid interval falls back to 30 seconds.
- Refreshes never overlap. The next refresh is scheduled after the current request cycle completes.
- Later failures retain the last successful topology and timestamp, report the real error, and continue polling for recovery.
- A configuration failure is shown with retry controls and uses the light theme plus the 30-second fallback interval unless the user has a saved theme override.
- Theme priority is the saved user override (`localStorage` key `docker-web-theme`) followed by the backend default. Toggling the theme updates Cytoscape styles without recreating the graph.
- The graph contains every container and network from the topology response. It renders only the supplied edges, so inactive containers remain isolated and unattached networks remain visible. Edges are labeled as Docker network membership, not traffic or reachability.
- Graph labels use compact display names for long generated Docker names; list and details views keep the full Docker names and IDs.
- Search trims the query and matches container names and raw Docker statuses case-insensitively. Matching containers are shown with their edge-connected networks and the induced supplied edges.
- The network filter matches containers by supplied membership edges to the selected full network ID. It is container-oriented: a selected empty network is still shown as an isolated node. The published-binding filter includes only running or paused containers with at least one Docker-reported non-null host port. Both filters compose with search by intersection.
- Network details show the full stable network ID, the legacy subnet, and every structured IPAM CIDR. Container details show separate IPv4/IPv6 attachment addresses, retained legacy addresses, and membership-edge status.
- Host port bindings are labeled `unknown`, `wildcard` (`0.0.0.0` or `::`), `loopback` (IPv4 `127/8` or `::1`), or `specific`. IPv6 host addresses use bracket formatting. Running/paused host bindings are counted as Docker-reported current bindings, never as proof of a listening service or reachability. Inactive containers show retained metadata and zero current bindings.
- The topology decoder accepts older responses that omit additive fields (`state`, `host_ip`, attachment `network_id`/`ipv4_address`/`ipv6_address`, and network `ipam_configs`) and validates their types whenever they are present.

## E2E graph bridge

Set `VITE_E2E=true` when starting the dev server or building the app to install a read-only bridge. It is absent otherwise.

```sh
VITE_E2E=true BACKEND_URL=http://127.0.0.1:8080 npm run dev
```

The bridge is available as `window.__DOCKER_WEB_GRAPH__`:

```ts
interface GraphBridge {
  readonly version: 1;
  getSnapshot(): GraphBridgeSnapshot;
}

interface GraphBridgeSnapshot {
  ready: boolean;
  nodeCount: number;
  edgeCount: number;
  nodes: GraphBridgeNodeSnapshot[];
  edges: GraphBridgeEdgeSnapshot[];
  selectedNodeRawIds: string[];
  selectedNodeInternalIds: string[];
  highlightedNodeRawIds: string[];
  highlightedNodeInternalIds: string[];
  highlightedEdgeInternalIds: string[];
  zoom: number;
  pan: { x: number; y: number };
  canvas: { width: number; height: number };
}

interface GraphBridgeNodeSnapshot {
  internalId: string;
  rawId: string;
  kind: 'container' | 'network';
  label: string;
  renderedPosition: { x: number; y: number };
  classes: string[];
}

interface GraphBridgeEdgeSnapshot {
  internalId: string;
  rawSourceId: string;
  rawTargetId: string;
  sourceInternalId: string;
  targetInternalId: string;
}
```

The arrays and objects returned by `getSnapshot()` are fresh copies of the actual rendered Cytoscape state. `renderedPosition` is relative to the graph canvas. Internal IDs are `container:<raw-id>` and `network:<raw-id>`. `ready` becomes true after the first topology projection is applied to Cytoscape, including an empty projection. The bridge has no mutation or imperative selection methods; tests should use real pointer clicks on the graph canvas.

## Stable UI selectors and accessibility

The main test IDs are:

- Application: `app-root`
- Requests and status: `loading-indicator`, `config-warning`, `config-retry-button`, `demo-mode-notice`, `topology-error`, `refreshing-indicator`, `refresh-button`, `last-success-timestamp`, `refresh-interval`
- Search: `search-input`, `search-clear`, `search-summary`
- Filters: `filter-toolbar`, `network-filter`, `network-filter-id`, `published-filter`, `filter-reset`
- Theme: `theme-toggle`
- Graph: `topology-graph`, `fit-graph-button`, `graph-membership-note`, `search-no-results`, `topology-graph-empty`, `topology-graph-loading`
- Lists: `topology-list`, `topology-list-containers`, `topology-list-networks`, `node-select-container-<raw-id>`, `node-select-network-<raw-id>`, `container-list-empty`, `network-list-empty`
- Details: `details-panel`, `details-empty`, `details-container`, `details-container-id`, `details-container-image`, `details-container-state`, `details-container-status`, `details-container-created`, `details-container-ports`, `details-container-port-<index>`, `details-container-binding-count`, `details-container-binding-state`, `details-container-networks`, `details-container-network-<name>`, `details-container-network-id-<name>`, `details-container-network-ipv4-<name>`, `details-container-network-ipv6-<name>`, `details-container-network-legacy-<name>`, `details-container-network-mac-<name>`, `details-container-network-membership-<name>`, `details-container-membership-note`, `details-network`, `details-network-id`, `details-network-driver`, `details-network-scope`, `details-network-subnet`, `details-network-cidrs`, `details-network-cidr-<index>`, `details-network-internal`, `details-network-members`, `details-network-member-<raw-container-id>`, `details-network-member-empty`, `details-network-membership-note`
- Counts: `count-total-containers`, `count-visible-containers`, `count-total-networks`, `count-visible-networks`

The search input is a labeled searchbox, list entries are toggle buttons with `aria-pressed`, notices use alert/status semantics, the graph is a labelled region, and the companion list plus details panel provide keyboard-accessible alternatives to canvas interaction.
