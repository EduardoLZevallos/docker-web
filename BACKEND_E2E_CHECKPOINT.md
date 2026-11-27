# Backend E2E final completion and handoff

Completed on 2026-11-23 with the production lifecycle fix. The 2026-11-22
completion and earlier pause checkpoint are historical.

## Location and status

- Repository/worktree: `/home/hubaxiya/projects/docker-web`
- Branch: `test/backend-e2e-contract`
- The earlier implementation is committed and pushed through `afe40c9`
  (`52a5589`, then harness hardening); the review, runner, and production
  lifecycle changes are uncommitted.
- Implementation and validation are complete for the current scope: production
  behavior plus regressions. The full serial suite is green; no expected failure
  remains.
- Legacy integration targets were not executed.
- No E2E test sessions or test-owned processes remain active.

## Completed work

1. **Harness:** `backend/tests/e2e_support/mod.rs` and test-only assets. Pinned
   Testcontainers 0.16.7 starts a privileged Docker 26.1.4 DinD container and a
   sibling backend container on the outer daemon. The runner and backend share a
   private Unix-socket directory; the backend is built from production source and
   serves real HTTP with the test `config.yaml` at `/app/config/config.yaml`.
   No Docker TCP listener is used.
   - The DinD data root is a fresh labeled named volume per scenario.
   - One immutable backend image is built once per suite invocation by the
     runner, which explicitly removes and verifies it after the test process
     exits.
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
7. **Follow-up review fixes:** mutation acknowledgements use a shared generation
   so waits require requests started after each mutation; backend pre-readiness
   failures share a stable category asserted by the invalid-config test; and
   `backend/tests/run_backend_e2e.sh` builds the image once, runs the suite,
   removes and verifies the image, and preserves the test exit status.
8. **Production lifecycle fix:** `backend/src/docker.rs` maps every
   `ContainerSummary` through one shared function. `list_containers` requests all
   states, while the existing running-only `list_running_containers` method is
   retained. `backend/src/api.rs` uses all-state queries for `/api/containers`
   and `/api/topology`; topology keeps every container as a node but adds edges
   only for Docker states `running`, `paused`, and `restarting`. Inactive
   containers preserve status, network attachments, and the existing JSON shape;
   the private edge-eligibility flag is not serialized. Two Docker-free library
   tests cover state mapping and metadata/JSON preservation.
9. **Final validation and records:** full runner, focused lifecycle, library,
   compile, syntax, and diff checks passed; current results and historical
   pre-fix evidence are recorded below.

## Final validation evidence

Validated on 2026-11-23 on branch `test/backend-e2e-contract`:

```sh
export PATH="/home/hubaxiya/.cargo/bin:$PATH"
backend/tests/run_backend_e2e.sh
backend/tests/run_backend_e2e.sh e2e_support::lifecycle:: --nocapture
cargo check --manifest-path backend/Cargo.toml --test backend_e2e
bash -n backend/tests/run_backend_e2e.sh
git diff --check
```

Results:

- Full serial runner: **39 passed (exit 0)** in **211.23 s**.
- Focused lifecycle run: **14 passed** in **81.41 s**.
- The two new Docker-free library tests for the shared Docker summary mapping
  passed.
- `cargo check --test backend_e2e`, `bash -n backend/tests/run_backend_e2e.sh`,
  and `git diff --check`: exit 0.
- Full log: `/tmp/opencode/backend_e2e_lifecycle_fix_full.log` (not committed).
- No owned image/tag, container, or volume remained per the Docker CLI.
- Production scope and regressions are delivered. Legacy integration targets
  were not executed.

## Historical pre-fix failure evidence

Before the production lifecycle fix, the six inactive-visibility contracts in
`e2e_support/lifecycle.rs` failed against the running-only API: created, stopped,
exit-zero, exit-nonzero, killed, and mixed-state visibility. The last full runs
were **33 passed; 6 failed** (39 tests) in 219.07 s, log
`/tmp/opencode/backend_e2e_review_final.log`, and **33 passed; 6 failed** in
213.66 s, log `/tmp/opencode/backend_e2e_runner_full.log`, with exit 101
expected. Independent Docker inspect showed CREATED/EXITED while
`/api/containers` and `/api/topology` omitted those containers; mixed state
omitted the created/exited nodes. These were desired-contract failures, not
harness failures. They are historical: after the production change the full
target passes all 39 tests.

## Cleanup and leak audit

Post-run audit on 2026-11-23:

- `docker ps -a` filtered for `docker-web-e2e`: none.
- `docker images` filtered for `docker-web-e2e`: none (the suite runner removed
  and verified the shared image after the process exited).
- No owned image/tag, container, or volume remained per the Docker CLI.
- No lingering `cargo test` or `backend_e2e` processes.
- No `docker prune` or removal of unrelated host resources was performed.

## Files changed

Uncommitted on top of the pushed branch head `afe40c9`:

- `backend/src/api.rs` — all-state `/api/containers` and `/api/topology`;
  topology edges filtered by Docker state.
- `backend/src/docker.rs` — one shared Docker summary mapping; all-state
  `list_containers`; retained running-only `list_running_containers`; two
  Docker-free library tests.
- `backend/tests/e2e_support/mod.rs` and
  `backend/tests/e2e_support/resilience/{concurrency,cleanup}.rs` — review fixes
  and stable startup failure category.
- `BACKEND_E2E_PLAN.md`, `BACKEND_E2E_CHECKPOINT.md`, and
  `backend/tests/e2e_assets/README.md` — current records.
- `backend/tests/run_backend_e2e.sh` (untracked) — shared image build/run/cleanup
  runner.

Previously committed in `52a5589`/`afe40c9`: the isolated harness, fixtures,
topology/lifecycle/resilience tests, and test assets. `backend/Cargo.lock`
remains gitignored. Legacy tests, frontend, and configuration files were not
changed; legacy integration targets were not executed.

## Remaining risks and limitations

- The full target is green; the earlier six inactive-visibility failures are
  historical and must not be restored as expected failures. Keep the visibility
  and active-state edge assertions enabled.
- The suite is serial and takes roughly 3.5 minutes; each scenario starts a
  fresh DinD daemon, while the backend image is shared within the process.
- It needs a privileged-capable outer Docker daemon, outbound registry access,
  and a local gitignored `backend/Cargo.lock` (`cargo build --locked` in the test
  image). The test-only inner daemon uses `https://mirror.gcr.io` with Docker Hub
  fallback.
- The suite runner removes and verifies the shared image after the test process
  exits. If the runner itself is killed before cleanup, a shared image may remain;
  it is tagged `docker-web-e2e:suite-<timestamp>-<pid>`.
- Validated only with outer Docker 29.1.3 and inner Docker 26.1.4. Legacy
  integration targets were not executed because they query the host daemon.
- Exit-code assertions for exits 0/7/137 now execute after visibility succeeds.
