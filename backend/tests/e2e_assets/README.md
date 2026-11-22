# Backend E2E test runner

Run the isolated backend acceptance target from the repository root:

```sh
export PATH="/home/hubaxiya/.cargo/bin:$PATH"
cargo test --manifest-path backend/Cargo.toml --test backend_e2e -- --test-threads=1
```

## Last validated result

Validated on 2026-11-22 on branch `test/backend-e2e-contract` (uncommitted):

- Result: **29 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out** in
  357.33 s. Exit code 101 is expected because of the six contract failures below.
- No harness, setup, outage/recovery, concurrency, or cleanup test failed.
- The validation log is preserved at `/tmp/opencode/backend_e2e_full.log` (not
  committed).

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
  parallel runs would overlap infrastructure.

## Isolated topology

Each test starts one privileged DinD container (`docker:26.1.4-dind`) and one
sibling backend container on the outer daemon. Both are named with
`docker-web-e2e-*` and owned by the test. The inner daemon uses a per-scenario
data root; the runner and backend share only a private bind-mounted Unix-socket
directory (`/e2e/docker.sock`). No Docker TCP listener is exposed. The backend is
built from [`Dockerfile`](Dockerfile) against the current production source and
receives [`config.yaml`](config.yaml) at `/app/config/config.yaml`; tests use
real HTTP against that binary. Inner fixture containers and networks carry a
run-ID label.

Cleanup runs after normal return, body panic, diagnostics failure, or
deterministic partial-startup failure. It removes labeled inner
containers/networks, the backend and DinD containers, the test image, socket, and
temporary config, then verifies the owned containers are gone. Cleanup errors
fail the test.

Legacy integration targets use the host Docker daemon and are outside this
command.

## Expected failures and interpretation

These tests intentionally assert the desired contract that inactive containers
remain visible with no topology edges. The current application lists only running
containers, so they are expected to fail and must stay enabled:

- `e2e_support::lifecycle::given_created_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_stopped_container_when_both_apis_read_then_it_remains_visible_without_topology_edges`
- `e2e_support::lifecycle::given_exit_zero_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_exit_nonzero_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_killed_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_mixed_lifecycle_states_when_topology_read_then_only_active_membership_edges_exist`

For the single-container cases, independent Docker inspect confirms
`created`/`exited`, but `/api/containers` and `/api/topology` omit the container.
The mixed-state case sees only running/paused containers and omits the
created/exited nodes. The exit-0, exit-7, and killed cases also assert API status
exit codes, but the visibility assertion fails first. These are contract
failures, not harness failures: the fixtures exist in the inner daemon and the
other scenarios in the same serial run pass.

Interpret any other failure as a regression or harness/setup problem. The
expected full-target outcome is exactly 29 passed and those six failures (exit
code 101). A focused rerun, for example, is:

```sh
cargo test --manifest-path backend/Cargo.toml --test backend_e2e \
  e2e_support::lifecycle::given_created_container_when_both_apis_read_then_it_is_visible_without_topology_edges \
  -- --exact --nocapture --test-threads=1
```

Failure output includes the last HTTP response, outer/inner Docker versions,
backend image ID, DinD/backend state, and container logs captured before
teardown.
