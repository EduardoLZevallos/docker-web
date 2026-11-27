# MVP baseline (block 1)

Recorded 2026-11-27 on branch `feature/frontend-backend-e2e` at
`ab59a4e1107a7c4216b8acfc5881c1757ffed9a4`.

This is a baseline/reproducibility checkpoint only. It does not change frontend
feature code, the Git index, or Git history. All pre-existing staged, unstaged,
and untracked user changes were preserved.

## Environment and prerequisites

- Node `v26.13.1` (`/home/hubaxiya/.local/bin/node`), npm `11.20.0`.
- Cargo/rustc `1.90.0`, rustfmt `1.8.0-stable`.
- Docker CLI/server `29.1.3`.
- Cached Playwright Chromium `chromium-1255`; `@playwright/test 1.65.1`.
- Port `5173` was free before each bind-owning run and after cleanup.
- Frontend dependencies, Playwright, and the backend `Cargo.lock` were already
  present locally.

## Baseline inventory

| File | Worktree mode | Git mode | Notes |
| --- | --- | --- | --- |
| `backend/tests/run_backend_e2e.sh` | `775` | `100755` | executable |
| `frontend/docker-web-ui/tests/run_demo.sh` | `775` | untracked | executable launcher |
| `frontend/docker-web-ui/tests/run_frontend_e2e.sh` | `775` | untracked | executable launcher |
| `backend/tests/e2e_assets/dind-entrypoint.sh` | `664` | `100644` | intentionally run as `/bin/sh /e2e-entrypoint.sh`, not executed directly |
| `backend/Cargo.lock` | `664` | untracked | previously ignored; now visible via the ignore exception below |

Baseline records present: `BACKEND_E2E_PLAN.md`, `BACKEND_E2E_CHECKPOINT.md`,
`backend/tests/e2e_assets/README.md`, and `frontend/docker-web-ui/README.md`.

## Baseline results

### Backend contract suite

```sh
backend/tests/run_backend_e2e.sh
```

- **39 passed; 0 failed; 2 ignored** (41 tests), **211.12 s** test time,
  runner elapsed **217 s**, **exit 0**.
- Log: [mvp_baseline_backend_20261127.log](/tmp/opencode/mvp_baseline_backend_20261127.log)
- Post-run audit: no `docker-web-e2e` containers, images, or volumes; port 5173
  free.

### Browser E2E suite

```sh
frontend/docker-web-ui/tests/run_frontend_e2e.sh
```

- Build/typecheck succeeded; Playwright: **18 passed (1.3 m)**; Rust wrapper:
  **1 passed** in **81.17 s**; **exit 0**, run ID
  `2768642-18ebc7ef9263e131`.
- Real Rust backend + private DinD; no API mocks or request interception.
- Log: [mvp_baseline_frontend_20261127.log](/tmp/opencode/mvp_baseline_frontend_20261127.log)
- Post-run audit: no `docker-web-e2e` containers, images, or volumes; no Vite
  preview process; port 5173 free.

### Live demo screenshots

```sh
VITE_E2E=true DEMO_HOLD_SECONDS=900 frontend/docker-web-ui/tests/run_demo.sh
```

`VITE_E2E=true` was added only to expose the existing read-only graph bridge for
verification; no source or launcher was changed.

- Demo run ID: `2802769-18ebcb946f20b45f`.
- Backend: `http://localhost:34790`; UI: `http://localhost:5173/`.
- API fixture proof for this exact run: **9 containers** and **3 demo networks**
  with prefix `e2e-2802769-18ebcb946f20b45f-demo-`; all names matched the exact
  expected set. Rendered graph: **15 nodes** (9 demo containers + 3 demo networks
  + built-in `bridge`/`host`/`none`) and **9 edges**.
- Evidence JSON: [mvp_baseline_visual_evidence_2802769-18ebcb946f20b45f.json](/tmp/opencode/mvp_baseline_visual_evidence_2802769-18ebcb946f20b45f.json)
- Demo log: [mvp_baseline_demo_20261127.log](/tmp/opencode/mvp_baseline_demo_20261127.log)
- Topology snapshot: [mvp_baseline_demo_topology_2802769-18ebcb946f20b45f.json](/tmp/opencode/mvp_baseline_demo_topology_2802769-18ebcb946f20b45f.json)

Screenshots (1440x900 desktop, 390x844 narrow):

| Screenshot | SHA-256 |
| --- | --- |
| [desktop light](/tmp/opencode/mvp_baseline_desktop_light_2802769-18ebcb946f20b45f.png) | `f3fb2f35af5a68aa784a8da1eff74deef3e9a367398d4fcb00a37c7701017d98` |
| [desktop dark](/tmp/opencode/mvp_baseline_desktop_dark_2802769-18ebcb946f20b45f.png) | `938b6e873b4a5742e44457fcc1540e324f6d2dcae51c646aa9a4a638704a337c` |
| [390 width light](/tmp/opencode/mvp_baseline_390_width_light_2802769-18ebcb946f20b45f.png) | `45ec231a859fca229dc52f0d2bb6a5c0cd3c3a0d6e46ca8bc68e9097c5f3f6bd` |

## Visual findings

- Desktop light: 15 nodes and 9 edges render; topology list shows 9 containers
  and 6 networks. **Graph node labels are effectively invisible in light mode**
  because white label text and a white text outline are drawn on a white canvas.
  This is a pre-existing visual defect, not changed by this block.
- Desktop dark: the same topology renders and labels are visible in white.
  Names in the right-hand list are truncated with ellipses, but the graph labels
  show them.
- 390 width: layout stacks vertically with the graph above the topology list and
  details panel; no horizontal overflow (`innerWidth/clientWidth/scrollWidth` all
  `390`); search, theme toggle, graph, list, and details remain present.
- Browser console reported no errors during capture.

## Clean snapshot checks

A source-only snapshot was copied to
`/tmp/opencode/mvp_baseline_snapshot_20261127T041239Z/repo` using the Git file
lists in read-only mode. It excluded `.env` files, `node_modules`, `dist`,
`target`, logs, test artifacts, and the generated `Cargo.lock`. Manifest:
[mvp_baseline_snapshot_20261127T041239Z/source_manifest.txt](/tmp/opencode/mvp_baseline_snapshot_20261127T041239Z/source_manifest.txt).

Checks in that clean copy all passed (log:
[mvp_baseline_snapshot_checks_20261127T041239Z.log](/tmp/opencode/mvp_baseline_snapshot_checks_20261127T041239Z.log)):

- `cargo generate-lockfile`: fresh dependency resolution succeeded (356
  registry packages, 357 lock entries including the workspace package).
- `cargo check --locked --all-targets`: passed.
- `cargo test --locked --lib`: **2 passed**.
- `bash -n` on all three launchers: passed.
- `npm ci`: 30 packages, passed.
- `npm run build:e2e` (includes typecheck): passed.

Generated build artifacts were removed after the checks; the source snapshot and
the check log remain under `/tmp/opencode`.

The freshly generated snapshot lockfile was **not identical** to the current
local `backend/Cargo.lock` (357 packages each; 13 packages differed by patch
version, for example `actix-http 3.22.0` vs `3.22.1` and
`zstd 0.14.4` vs `0.14.5`). This confirms that leaving the lockfile ignored makes
exact dependency resolution non-reproducible.

## Reproducibility changes

- `.gitignore` now has a narrow exception after `Cargo.lock`:
  `!/backend/Cargo.lock`. `backend/Cargo.lock` now appears as an untracked file
  and is **not staged**.
- The three launchers already had executable modes; no mode change was needed.
- No frontend feature source was edited.

## Cleanup status

After the demo capture and final audit:

- Port `5173`: no listener.
- Owned `docker-web-e2e` containers: none.
- Owned `docker-web-e2e` images: none.
- Owned `docker-web-e2e` volumes: none.
- No leftover backend E2E, demo, Vite preview, or capture processes.
- All discovered Docker fixtures were created inside the private DinD daemon;
  no real host Docker fixtures were used.

## Blockers and caveats

- No blocker prevented the baseline runs.
- The light-theme graph-label contrast issue needs a separate frontend fix.
- Lockfile patch drift remains until `backend/Cargo.lock` is tracked; this block
  only makes it visible and does not stage it.
- Screenshots and logs are in `/tmp/opencode`, so they are not durable in Git.

## Files changed by this block

- `.gitignore` (Cargo.lock ignore exception)
- `MVP_BASELINE.md` (this record)
- `backend/Cargo.lock` is now unignored/untracked but its contents were not
  changed or staged.
