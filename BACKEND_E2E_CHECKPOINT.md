# Backend E2E final completion and handoff

Completed on 2026-11-22. This replaces the earlier pause checkpoint.

## Location and status

- Repository/worktree: `/home/hubaxiya/projects/docker-web`
- Branch: `test/backend-e2e-contract`
- All changes are uncommitted. **No commit was made.**
- Implementation and validation are complete. The only failures are the six
  expected inactive-visibility contract failures; no unexpected failure remains
  unresolved.
- No E2E test sessions or test-owned processes remain active.

## Completed work

1. **Harness:** `backend/tests/e2e_support/mod.rs` and test-only assets. Pinned
   Testcontainers 0.16.7 starts a privileged Docker 26.1.4 DinD container and a
   sibling backend container on the outer daemon. The runner and backend share a
   private Unix-socket directory; the backend is built from the production source
   and serves real HTTP with the test `config.yaml` at `/app/config/config.yaml`.
   No Docker TCP listener is used. Startup, diagnostics, cleanup, and
   deterministic partial-startup faults are panic-safe.
2. **Topology:** `backend/tests/e2e_support/topology.rs`. Seven matrix cases,
   uneven `[0,1,4]`, 8-by-4, multihoming, network membership mutations,
   same-name recreation, repeated reads, bridge/internal/host/none/shared
   namespace modes, and TCP/UDP port metadata. Expected IDs/edges come from
   independent Docker inspection and create responses, not backend output.
3. **Lifecycle:** `backend/tests/e2e_support/lifecycle.rs`. 14 scenarios covering
   created, running, stopped, exit 0/7/137, killed, paused, unpaused, restarted,
   restarting, removed, mixed states, and health transitions. Exit-code status
   assertions are present for exits 0/7/137.
4. **Resilience:** `backend/tests/e2e_support/resilience.rs`. Same-DinD outage
   and recovery without restarting the backend; stop/start/restart/remove
   identity/edge behavior; four concurrent readers overlapping bounded
   mutations with snapshot validity and exact convergence checks; cleanup proofs
   for normal return, body panic, closure-invocation panic, and partial startup.
5. **Records:** `backend/tests/e2e_assets/README.md`, the final Progress section
   of `BACKEND_E2E_PLAN.md`, and this handoff.

## Final validation evidence

Commands:

```sh
export PATH="/home/hubaxiya/.cargo/bin:$PATH"
rustfmt --check --edition 2024 backend/tests/backend_e2e.rs \
  backend/tests/e2e_support/mod.rs backend/tests/e2e_support/topology.rs \
  backend/tests/e2e_support/lifecycle.rs backend/tests/e2e_support/resilience.rs
cargo check --manifest-path backend/Cargo.toml --test backend_e2e
cargo test --manifest-path backend/Cargo.toml --test backend_e2e -- --test-threads=1
```

Results:

- `rustfmt --check`: exit 0, no files needed formatting.
- `cargo check --test backend_e2e`: exit 0.
- Full serial run: **29 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out**;
  finished in **357.33 s**; exit code 101 expected.
- Full log: `/tmp/opencode/backend_e2e_full.log`.
- No additional failures occurred, so no focused rerun was required.

The six expected failures are:

- `e2e_support::lifecycle::given_created_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_stopped_container_when_both_apis_read_then_it_remains_visible_without_topology_edges`
- `e2e_support::lifecycle::given_exit_zero_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_exit_nonzero_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_killed_container_when_both_apis_read_then_it_is_visible_without_topology_edges`
- `e2e_support::lifecycle::given_mixed_lifecycle_states_when_topology_read_then_only_active_membership_edges_exist`

Each failed because the current running-only API omitted inactive containers:
independent Docker inspect showed CREATED or EXITED, but `/api/containers` and
`/api/topology` did not contain the fixture. The mixed-state test expected
running, paused, created, and exited nodes but received only running and paused.
Harness setup, Docker inspection, and every other scenario in the same run
succeeded, so these are desired-contract failures, not fixture failures.

## Cleanup and leak audit

- `docker ps -a` filtered for `docker-web-e2e`: none.
- `docker images` filtered for `docker-web-e2e`: none.
- `docker volume ls`: unchanged from the pre-run baseline of three pre-existing
  anonymous volumes:
  `1a713c96912ddd74a2bc28ec256068c1db4dbba7f1ab4b4f401b3324e2158f3d`,
  `951aaf5129f12fef478008f013c9fbee7427f392abdfe2758b6c75fb0e40e62b`,
  `ab07c73244214acc0ca02fdd739091e33e479d915b27c476698bb90b49e9f6f8`.
- No lingering `cargo test` or `backend-e2e` processes.
- This run left no relevant temporary socket/config directories. Older
  `/tmp/.tmp*` directories from earlier sessions predate this run (19:00-19:17
  UTC) and were left untouched.
- No `docker prune` or removal of unrelated host resources was performed.

## Files changed

Modified (tracked):

- `backend/Cargo.toml` — dev-dependencies only: added `futures-util` and
  `reqwest`; pinned `testcontainers = "=0.16.7"`.

Untracked (authorized E2E work):

- `BACKEND_E2E_PLAN.md`
- `BACKEND_E2E_CHECKPOINT.md`
- `backend/tests/backend_e2e.rs`
- `backend/tests/e2e_support/{mod,topology,lifecycle,resilience}.rs`
- `backend/tests/e2e_assets/{Dockerfile,README.md,config.yaml,dind-entrypoint.sh}`

`backend/Cargo.lock` is generated and remains ignored by the existing
`.gitignore`. No `backend/src`, legacy test, frontend, or configuration files
were changed.

## Remaining risks and limitations

- The six inactive-visibility tests must stay enabled and will keep the target
  at exit code 101 until the application lists created/stopped/exited/killed
  containers with no active topology edges. Do not weaken or ignore them.
- The suite is serial and takes about six minutes; each scenario builds a
  release backend image and starts a fresh DinD daemon.
- It needs a privileged-capable outer Docker daemon, outbound registry access,
  and a local gitignored `backend/Cargo.lock` (`cargo build --locked` in the test
  image). The test-only inner daemon uses `https://mirror.gcr.io` with Docker Hub
  fallback.
- Validated only with outer Docker 29.1.3 and inner Docker 26.1.4. Legacy
  integration targets are excluded because they query the host daemon.
- Exit-code assertions for exits 0/7/137 exist but are not reached while the
  visibility assertion fails first.
