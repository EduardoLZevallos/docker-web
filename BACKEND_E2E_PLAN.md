# Backend E2E implementation plan

## Execution constraints

- Work sequentially in the existing worktree on `test/backend-e2e-contract`.
- Hand each logical implementation block to Luna at medium effort, then review it before starting the next block.
- Change tests, test assets, documentation, and development dependencies only. Do not change application behavior to make assertions pass.
- Test the desired contract, including visibility of inactive containers. Unsupported behavior must fail the main E2E target.
- Implementation started at approximately 2026-11-22 16:41 UTC (branch creation). Stop by **2026-11-22 21:11 UTC**, retaining 10% of the five-hour limit. If unfinished, write `BACKEND_E2E_CHECKPOINT.md` with changed files, results, running resources/processes, and exact next steps before stopping.

## 1. Isolated harness

Testcontainers owns a privileged Docker-in-Docker daemon and a separate backend container. Both infrastructure containers live outside the discovered daemon. The runner and backend share the inner daemon's Unix-socket directory; no Docker TCP listener is needed. Supply generated YAML at the backend's existing `config/config.yaml` path. Use real HTTP to the actual backend binary, not an in-process Actix test service.

Use pinned compatible images, fresh daemon storage per scenario, bounded readiness polling, diagnostics before teardown, and cleanup on assertion failure or panic. Fail clearly for unavailable prerequisites. Never prune or manipulate unrelated host workloads.

## 2. Topology and metadata

Cover `(custom networks, containers per network)` pairs `(0,0)`, `(1,0)`, `(1,1)`, `(1,4)`, `(3,0)`, `(3,1)`, `(3,4)`, uneven populations `[0,1,4]`, and a bounded 8-by-4 case. Preserve built-in networks in empty environments.

Assert exact IDs and topology edges against fixture definitions and independently inspected Docker IDs; reject duplicates and dangling references. Include multi-network containers, isolated groups, connect/disconnect, empty-network deletion, removal and same-name recreation. Verify Docker-appropriate metadata for bridge/internal/host/none/shared-network-namespace modes, optional addresses, exposed/published TCP and UDP ports. Do not derive expected results from backend responses or use the intentionally unpopulated network membership list as an oracle.

## 3. Lifecycle contracts

Use independent tests for created, running, gracefully stopped, successful/failed exit, killed, paused, unpaused, restarted, restarting, and removed states, plus mixed-state populations and transition sequences. Inactive containers remain visible but have no active topology edges; paused containers retain attachments. Stop/start and restart preserve identity; recreation changes it. Existing status strings suffice: do not require new API fields or exact uptime wording.

Establish states through independent Docker inspection. Use controlled health transitions and a deliberate restart-loop fixture with a bounded, inspected observation window. Created and exited visibility is expected to fail against the current running-only implementation; do not weaken the contract.

## 4. Resilience and acceptance

Stop and restart only the same isolated DinD container, preserving its data and socket directory. During outage expect discovery HTTP 503, health HTTP 200 with degraded/Docker-unreachable status, and readable configuration. Recover without restarting the backend; inspect surviving state and create a new marker workload to prove fresh discovery.

Exercise concurrent readers during mutations without assuming atomic snapshots. Assert exact sets after convergence. Capture logs, versions/digests, inspections, and last HTTP responses; distinguish infrastructure, behavior, and cleanup failures.

Run the new target serially, avoiding legacy tests that query the host daemon:

```sh
cargo test --manifest-path backend/Cargo.toml --test backend_e2e -- --test-threads=1
```

Completion means executable coverage, verified isolation/cleanup, and actionable failure evidence. A green suite is not required where production behavior does not yet satisfy the requested contract.

## Progress

- Planning: four sequential Luna/medium review handoffs completed and reviewed.
- Branch `test/backend-e2e-contract` created in `/home/hubaxiya/projects/docker-web`; application files unchanged.
- Block 1 implemented: isolated DinD/backend harness, test-only images/configuration, real HTTP, panic capture, cleanup. Rust 1.90 installed. `cargo check --test backend_e2e` and live smoke test passed.
- Block 2 implemented and reviewed: independent name/ID/edge oracles, seven matrix cases, uneven/8-by-4, network mutations and modes, metadata/ports, and recreation. Compilation passed; live topology cases passed after correcting two test expectations.
- Block 3 implemented and reviewed: 14 independent lifecycle scenarios. Compilation passed; live serial run reported 8 passes and 6 desired-contract failures (created, stopped, exit-zero, exit-nonzero, killed, mixed-state visibility). Pause/unpause, restart/restarting, removal, and health scenarios passed.
- Block 4a validated: the test-only DinD entrypoint now delegates to the official image entrypoint with an explicit Unix-socket dockerd command, and the harness uses a checked 30-second outer stop. Three consecutive focused outage/recovery runs passed, as did the stop/start/restart/remove sequence. No leftover E2E resources.
- Block 4b implemented: four concurrent readers now overlap a bounded connect/disconnect/create/stop/restart/remove sequence, validate every 200 snapshot for duplicate IDs/edges and same-response edge references, and converge to an independently derived exact fixture topology. Focused test passed three consecutive serial runs with no 503s; existing sequence test still passed.
- Block 4c implemented: closure-invocation panics and diagnostics failures now cannot skip cleanup; teardown catches container-removal panics and verifies owned containers/images/volumes; deterministic partial-startup faults exercise real cleanup. Lifecycle tests now assert API exit-code status for exits 0/7/137, all lifecycle names follow GIVEN/WHEN/THEN, and network-mode metadata is checked against independent Docker inspection. Lifecycle baseline remained 8 passed / 6 expected inactive-visibility failures; cleanup probes passed three repeated serial runs.
- Block 4d (final validation and records) completed on 2026-11-22:
  - `rustfmt --check --edition 2024` passed on `backend/tests/backend_e2e.rs` and all four `backend/tests/e2e_support/*.rs` files; no formatting changes were needed.
  - `cargo check --manifest-path backend/Cargo.toml --test backend_e2e` passed.
  - Full serial run `cargo test --manifest-path backend/Cargo.toml --test backend_e2e -- --test-threads=1` finished in **357.33 s** with **29 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out** (exit 101, expected). Log: `/tmp/opencode/backend_e2e_full.log`.
  - The only failures were the six inactive-visibility contracts (created, stopped, exit-0, exit-7, killed, mixed state). Independent Docker inspect confirmed CREATED/EXITED while the running-only API omitted the node; the mixed-state case omitted the created/exited pair. No additional failures occurred, so no focused flake reruns were needed.
  - Post-run leak audit: no `docker-web-e2e` containers or images; the three pre-existing anonymous volumes were unchanged; no lingering cargo/backend-e2e processes.
  - Authorized-diff audit: only `backend/Cargo.toml` is modified (`futures-util` and `reqwest` added as dev-dependencies; `testcontainers` pinned to `=0.16.7`). All other changes are authorized untracked E2E docs/tests/assets. `backend/Cargo.lock` remains gitignored. No `backend/src`, legacy test, frontend, or configuration changes.
  - Records updated: `backend/tests/e2e_assets/README.md`, this Progress section, and `BACKEND_E2E_CHECKPOINT.md`. **No commit was made.**

## Limitations

- The target is not green: the six inactive-container visibility tests fail against the current running-only implementation. They must remain enabled as the desired contract, and the suite exits 101 with exactly those failures until the application lists inactive containers without topology edges.
- The target is serial and relatively slow (~6 minutes) because each scenario builds a release backend image and starts a fresh DinD daemon.
- It requires a privileged-capable outer Docker daemon, network access for image pulls, and a local gitignored `backend/Cargo.lock`. Inner pulls use the test-only `https://mirror.gcr.io` registry mirror with Docker Hub fallback.
- Validated with outer Docker 29.1.3 and inner Docker 26.1.4; other versions are untested. Legacy integration targets are excluded because they query the host daemon.
- Exit-code assertions for exits 0/7/137 exist but are not reached while the inactive-visibility assertion fails first.
