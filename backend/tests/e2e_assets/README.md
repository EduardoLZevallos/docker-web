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

## Last validated result

Validated on 2026-11-23 on branch `test/backend-e2e-contract` through
`backend/tests/run_backend_e2e.sh`:

- Full serial target: **39 passed (exit 0)** in **211.23 s**.
- Focused lifecycle run: **14 passed** in **81.41 s**.
- The two new Docker-free library tests for the shared Docker summary mapping
  passed.
- `cargo check --test backend_e2e`, `bash -n backend/tests/run_backend_e2e.sh`,
  and `git diff --check` passed.
- The runner removed and verified the shared image; no owned image/tag,
  container, or volume remained per the Docker CLI.
- The full-validation log is preserved at
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
