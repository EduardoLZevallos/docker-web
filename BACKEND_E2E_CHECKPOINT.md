# Backend E2E final completion and handoff

Completed on 2026-11-22, including the PR #9 review round. This replaces the
earlier pause checkpoint.

## Location and status

- Repository/worktree: `/home/hubaxiya/projects/docker-web`
- Branch: `test/backend-e2e-contract`
- The earlier implementation is committed and pushed as `52a5589`; the PR #9
  review changes are currently uncommitted.
- Implementation and validation are complete for the current scope. The only
  failures are the six expected inactive-visibility contract failures; no
  unexpected failure remains unresolved.
- No E2E test sessions or test-owned processes remain active.

## Completed work

1. **Harness:** `backend/tests/e2e_support/mod.rs` and test-only assets. Pinned
   Testcontainers 0.16.7 starts a privileged Docker 26.1.4 DinD container and a
   sibling backend container on the outer daemon. The runner and backend share a
   private Unix-socket directory; the backend is built from production source and
   serves real HTTP with the test `config.yaml` at `/app/config/config.yaml`.
   No Docker TCP listener is used.
   - The DinD data root is a fresh labeled named volume per scenario.
   - One immutable backend image is built per suite invocation and removed by a
     detached janitor after the test process exits.
   - Startup, diagnostics, and cleanup are panic-safe. A startup guard removes
     owned containers and the data volume if Testcontainers panics before a
     harness exists.
   - Recovery uses `timeout_at` for both inspect and ping, so the 60-second
     recovery bound is hard even though the normal inner client has a 120-second
     timeout.
2. **Topology:** `backend/tests/e2e_support/topology.rs` and shared
   `fixtures.rs`. Seven matrix cases, uneven `[0,1,4]`, 8-by-4, multihoming,
   network membership mutations, same-name recreation, repeated reads,
   bridge/internal/host/none/shared namespace modes, and TCP/UDP port metadata.
   Expected IDs/edges come from independent Docker inspection and create
   responses, not backend output. The size matrix now checks `/api/containers`,
   `/api/networks`, and `/api/topology` cross-endpoint consistency.
3. **Lifecycle:** `backend/tests/e2e_support/lifecycle.rs`. 14 scenarios covering
   created, running, stopped, exit 0/7/137, killed, paused, unpaused, restarted,
   restarting, removed, mixed states, and health transitions. Exit-code status
   assertions are present for exits 0/7/137. The restart-loop scenario checks
   both container visibility and topology attachment edges while inspect confirms
   the restarting window.
4. **Resilience:** `backend/tests/e2e_support/resilience/{recovery,concurrency,cleanup}.rs`.
   - Same-DinD outage and recovery without restarting the backend, plus an
     unresponsive-but-connected daemon simulated by pausing only DinD; health and
     discovery responses must stay bounded and recovery must not restart the
     backend.
   - Stop/start/restart/remove identity/edge behavior with only HTTP 200
     discovery responses during ordinary transitions.
   - Four concurrent readers overlap bounded mutations; transient identity is
     registered behind a reader gate before it can be observed, readers
     acknowledge mutations, and final exact topology converges.
   - Cleanup proofs for normal return, body panic, closure-invocation panic,
     deterministic partial startup, and real startup failures (missing
     Dockerfile build, invalid backend config, missing backend image panic).
5. **Shared support:** `fixtures.rs` provides image/network/container helpers,
   named fixture structs, independent `TopologySnapshot` parsing/uniqueness/
   referential-integrity/exact-set checks, timestamp/error assertions, and
   cross-endpoint consistency. `resilience.rs` was split into the three modules
   above.
6. **Records:** `backend/tests/e2e_assets/README.md`, the final Progress section
   of `BACKEND_E2E_PLAN.md`, and this handoff.

## Final validation evidence

Commands:

```sh
export PATH="/home/hubaxiya/.cargo/bin:$PATH"
rustfmt --check --edition 2024 backend/tests/backend_e2e.rs \
  backend/tests/e2e_support/mod.rs backend/tests/e2e_support/fixtures.rs \
  backend/tests/e2e_support/topology.rs backend/tests/e2e_support/lifecycle.rs \
  backend/tests/e2e_support/resilience/mod.rs \
  backend/tests/e2e_support/resilience/recovery.rs \
  backend/tests/e2e_support/resilience/concurrency.rs \
  backend/tests/e2e_support/resilience/cleanup.rs
cargo check --manifest-path backend/Cargo.toml --test backend_e2e
cargo test --manifest-path backend/Cargo.toml --test backend_e2e -- --test-threads=1
```

Results:

- `rustfmt --check`: exit 0.
- `cargo check --test backend_e2e`: exit 0, no warnings.
- Full serial run: **33 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out**
  (39 tests); finished in **219.07 s**; exit code 101 expected.
- Full log: `/tmp/opencode/backend_e2e_review_final.log`.
- Focused reruns also passed: concurrency repeated twice more; cleanup/fault
  suite passed repeatedly; outage/recovery, sequence, unresponsive-daemon,
  restarting-topology, and invalid-config probes passed.

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
- `docker images` filtered for `docker-web-e2e`: none (the suite janitor removed
  the shared image after the process exited).
- The labeled DinD data volumes were removed; no owned volume remained.
- Pre-existing unrelated anonymous host volumes were untouched.
- No lingering `cargo test`, `backend_e2e`, or janitor processes.
- This run left no relevant temporary socket/config directories.
- No `docker prune` or removal of unrelated host resources was performed.

## Files changed

Modified (tracked):

- `backend/Cargo.toml` — dev-dependencies only: added `futures-util` and
  `reqwest`; pinned `testcontainers = "=0.16.7"`.
- New review-round test changes are uncommitted on top of `52a5589`.

Untracked/updated (authorized E2E work):

- `BACKEND_E2E_PLAN.md`
- `BACKEND_E2E_CHECKPOINT.md`
- `backend/tests/backend_e2e.rs`
- `backend/tests/e2e_support/{fixtures,mod,topology,lifecycle}.rs`
- `backend/tests/e2e_support/resilience/{mod,recovery,concurrency,cleanup}.rs`
- `backend/tests/e2e_assets/{Dockerfile,README.md,config.yaml,dind-entrypoint.sh}`

`backend/Cargo.lock` is generated and remains ignored by the existing
`.gitignore`. No `backend/src`, legacy test, frontend, or configuration files
were changed.

## Remaining risks and limitations

- The six inactive-visibility tests must stay enabled and will keep the target
  at exit code 101 until the application lists created/stopped/exited/killed
  containers with no active topology edges. Do not weaken or ignore them.
- The suite is serial and takes roughly four minutes; each scenario starts a
  fresh DinD daemon, while the backend image is shared within the process.
- It needs a privileged-capable outer Docker daemon, outbound registry access,
  and a local gitignored `backend/Cargo.lock` (`cargo build --locked` in the test
  image). The test-only inner daemon uses `https://mirror.gcr.io` with Docker Hub
  fallback.
- The suite image janitor is a detached shell process that removes the image
  after this test process exits. If the process group is killed in an unusual
  way, a shared image may remain; it is tagged `docker-web-e2e:suite-<pid>`.
- Validated only with outer Docker 29.1.3 and inner Docker 26.1.4. Legacy
  integration targets are excluded because they query the host daemon.
- Exit-code assertions for exits 0/7/137 exist but are not reached while the
  visibility assertion fails first.
