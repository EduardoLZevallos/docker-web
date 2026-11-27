# Backend E2E test runner

Run the isolated backend acceptance target from the repository root through the
suite runner:

```sh
export PATH="/home/hubaxiya/.cargo/bin:$PATH"
backend/tests/run_backend_e2e.sh
```

The runner builds the shared backend image once, runs the serial target, then
removes and verifies the image. It exits with the test status, or with a failure
if cleanup cannot be verified after a successful test run. Extra arguments are
forwarded to libtest, for example
`backend/tests/run_backend_e2e.sh <test-filter> --exact --nocapture`.

## Interactive topology demo

Seed three isolated networks with 2, 3, and 4 fake containers, plus one
multi-homed container attached to the frontend and backend networks. The
multi-homed container publishes one loopback-only and one wildcard host binding
(`127.0.0.1` and `0.0.0.0`) on Docker-assigned ports. The demo prints a stable
run token and exact fixture names/IDs:

```sh
DEMO_HOLD_SECONDS=600 backend/tests/run_backend_e2e.sh demo --ignored --nocapture
```

The test prints the `localhost` URLs for `/api/topology`, `/api/containers`,
and `/api/networks`, then holds the topology open until the timeout or Ctrl+C.
The readiness record includes the run token, backend origin, and fixture
network/container manifests for inspecting the exact run. The normal harness
cleanup removes all seeded containers and networks.

To browse the same demo through the real UI, use the one-command launcher from
the repository root:

```sh
DEMO_HOLD_SECONDS=600 frontend/docker-web-ui/tests/run_demo.sh
```

It starts this ignored demo through the suite runner with a structured
`DEMO_READY_FILE`, waits for the JSON readiness record, starts Vite dev with
`BACKEND_URL` set to the exact backend origin, prints
`http://localhost:5173`, and stops the UI when the hold expires or on Ctrl+C.
The launcher then waits for the backend harness to finish its normal Docker
cleanup. It never prunes unrelated Docker resources. Launcher-only overrides:
`DEMO_HOLD_SECONDS` (default `600`), `DEMO_STARTUP_TIMEOUT_SECONDS` (default
`300`), `DEMO_VITE_STARTUP_TIMEOUT_SECONDS` (default `90`), and
`DEMO_SHUTDOWN_TIMEOUT_SECONDS` (default `180`).

## Browser E2E against the private DinD fixture

The ignored `playwright_frontend_e2e` target uses the same `E2eHarness::run`
startup, diagnostics, and cleanup path as the backend contract suite. It builds
the React UI with `VITE_E2E=true`, serves it through Vite preview on port 5173,
and runs Playwright against the real backend HTTP proxy and private inner DinD
daemon. There are no API mocks or request interception.

```sh
frontend/docker-web-ui/tests/run_frontend_e2e.sh
```

The shell runner resolves `frontend/docker-web-ui`, typechecks/builds the E2E
bundle once with `npm --prefix`, then invokes:

```sh
backend/tests/run_backend_e2e.sh playwright_frontend_e2e --ignored --exact --nocapture
```

It exports `E2E_UI_PREBUILT=true` so Playwright previews the existing build
instead of running a second build. Playwright records the preview process group
in the harness temporary directory; the harness stops that group after normal
completion, failure, timeout, or SIGINT/SIGTERM so interrupted runs leave no
preview server behind.

Prerequisites are Node 26.13.1 (`~/.local/bin` or
`~/.local/share/node/node-v26.13.1-linux-x64/bin`), `npm ci` in
`frontend/docker-web-ui`, and a cached Chromium installed with
`npx playwright install chromium`. Tests fail with an explicit message rather
than installing dependencies or browsers implicitly.

The Rust harness starts its browser instance with `discovery_interval: 5` and
accepts the suite theme through `E2E_FRONTEND_THEME=light|dark` (default
`light`); the shared 30-second fixture used by backend tests is unchanged. The
Playwright process receives `BACKEND_URL`, `E2E_DOCKER_SOCKET`, `E2E_RUN_ID`,
`E2E_DIND_ID`, `E2E_BACKEND_ID`, `E2E_RUN_LABEL`, and, when configured,
`E2E_OUTER_DOCKER_HOST`. Browser-created fixture containers and networks must
carry the harness run label `com.docker-web.e2e.run=<E2E_RUN_ID>` so the
standard cleanup can verify their removal. Playwright uses one worker, zero
retries, Chromium only, Vite preview with `BACKEND_URL`, and retains traces and
screenshots on failure.

## Last validated result

Validated on 2026-11-26 on branch `feature/frontend-backend-e2e` through
`backend/tests/run_backend_e2e.sh`:

- Full serial target: **39 passed (exit 0)** in **208.79 s**; log
  `/tmp/opencode/backend_e2e_full_final2.log` (not committed).
- Focused lifecycle run from the earlier validation: **14 passed** in
  **81.41 s**.
- The two new Docker-free library tests for the shared Docker summary mapping
  passed.
- `cargo check --test backend_e2e`, `bash -n backend/tests/run_backend_e2e.sh`,
  and `git diff --check` passed.
- The runner removed and verified the shared image; no owned image/tag,
  container, or volume remained per the Docker CLI.
- The earlier full-validation log is preserved at
  `/tmp/opencode/backend_e2e_lifecycle_fix_full.log` (not committed).

Production scope and regressions were validated. Legacy integration targets
were not executed.

## Prerequisites

- Linux Docker host; the outer daemon must allow privileged containers and bind
  mounts.
- Rust 1.90.0 (`cargo`, `rustfmt`) on `PATH`. The validated environment used
  `/home/hubaxiya/.cargo/bin`.
- A working Docker CLI for the test process.
- Outbound registry access on first run. The test-only DinD entrypoint sets
  `--registry-mirror=https://mirror.gcr.io` to avoid anonymous Docker Hub rate
  limits; Docker falls back to Docker Hub when the mirror is unavailable. This
  affects only the inner fixture daemon, not host Docker configuration.
- `backend/Cargo.lock` must exist locally (it is intentionally gitignored). The
  test image builds with `cargo build --locked`; generate the lockfile with a
  normal backend build first if it is missing.
- Run serially. Each scenario controls its own daemon and socket lifecycle, so
  parallel runs would overlap infrastructure. Invoke the target through
  `backend/tests/run_backend_e2e.sh`; the harness requires the runner-provided
  `DOCKER_WEB_E2E_SUITE_IMAGE` and does not build or detach a cleanup process.

## Isolated topology

Each test starts one privileged DinD container (`docker:26.1.4-dind`) and one
sibling backend container on the outer daemon. Both are named with
`docker-web-e2e-*` and owned by the test. The inner daemon uses a fresh labeled
named volume for `/var/lib/docker`; the runner and backend share only a private
bind-mounted Unix-socket directory (`/e2e/docker.sock`). No Docker TCP listener
is exposed. The backend image is built once per suite invocation from
[`Dockerfile`](Dockerfile) against the current production source and receives
[`config.yaml`](config.yaml) at `/app/config/config.yaml`; tests use real HTTP
against that binary. Inner fixture containers and networks carry a run-ID label.
The fixture daemon enables IPv6 for user-defined networks with a private
`fd00:dead:beef::/64` default-bridge range so dual-stack attachment and host
binding coverage uses real Docker data without host network configuration.

Cleanup runs after normal return, body panic, diagnostics failure, a real
startup panic, or deterministic startup-failure injection. It removes labeled
inner containers/networks, the backend and DinD containers, the named DinD data
volume, socket, and temporary config, then verifies the owned containers and
volume are gone. Cleanup errors fail the test. A startup guard removes owned
containers and the data volume even when Testcontainers panics before a harness
exists. The shared backend image is built once by the suite runner and explicitly
removed and verified after the test process exits, so scenarios reuse it without
leaving an image behind.

Legacy integration targets use the host Docker daemon and are outside this
command.

## Lifecycle contract and interpretation

Production now uses one shared Docker summary mapping and all-state queries for
`/api/containers` and `/api/topology`. Topology keeps every container as a node
but adds edges only for Docker states `running`, `paused`, and `restarting`.
Inactive containers keep their status, network attachments, and the existing
JSON shape; only the private edge-eligibility flag is omitted from JSON. The
existing running-only Rust method is retained.

Interpret any failure as a regression or harness/setup problem. The expected
full-target outcome is **39 passed**, exit 0.

### Historical pre-fix evidence

Before the production change, six inactive-visibility contracts failed against
the then running-only API: **33 passed; 6 failed** in 213.66 s, exit 101, log
`/tmp/opencode/backend_e2e_runner_full.log`. The created, stopped, exit-zero,
exit-nonzero, killed, and mixed-state cases confirmed CREATED/EXITED with Docker
inspect while `/api/containers` and `/api/topology` omitted those containers.
These were contract failures, not harness failures, and are now historical; the
full target passes all 39 tests.

## Additional contract coverage

- Lifecycle checks cover created, running, stopped, exit 0/7/137, killed, paused,
  unpaused, restarting, removed, mixed states, and health transitions. Inactive
  containers remain visible without edges; running/paused/restarting containers
  keep their attachment edges.
- The topology size matrix runs against `/api/containers`, `/api/networks`, and
  `/api/topology`, checking stable-state cross-endpoint consistency.
- Concurrent readers require HTTP 200 throughout ordinary mutations, pre-register
  transient fixture identity behind a reader gate, and wait for observation
  acknowledgements instead of fixed sleeps. Daemon-outage 503s stay confined to
  the recovery scenario.
- Restarting containers are checked through both `/api/containers` and
  `/api/topology`, including the preserved network attachment edge.
- An unresponsive-but-connected daemon is simulated by pausing only the isolated
  DinD container; health and discovery responses must stay bounded, and recovery
  must not restart the backend.
- Real startup failures cover a missing Dockerfile build, an invalid backend
  config that exits the binary, and a missing backend image that makes
  Testcontainers panic. Each must leave no owned container or data volume.

A focused lifecycle rerun is:

```sh
backend/tests/run_backend_e2e.sh e2e_support::lifecycle:: --nocapture
```

Failure output includes the last HTTP response, outer/inner Docker versions,
backend image ID, DinD/backend state, and container logs captured before
teardown.
