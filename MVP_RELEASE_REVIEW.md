# MVP release review (block 6)

Reviewed 2026-11-27 on branch `feature/frontend-backend-e2e` at
`ab59a4e1107a7c4216b8acfc5881c1757ffed9a4`. Frontend BLOCK4 was already
integrated. Worktree changes were left unstaged; no staging, commits, resets, or
history changes were made. Final production smoke is owned by another agent and
was not run here.

## Scope and changes

- Reviewed the integrated filter toolbar, subnet-overlap panel, graph, list,
  details, themes, and narrow layout against the real live demo fixtures.
- Fixed the reported nondeterministic `filters.spec.ts` comparison. The old
  helper compared sorted port text but then checked `data-binding-kind` by
  expected index, which assumed Docker returned bindings in the sorted order.
  `expectContainerDetails` now compares exact sorted `(row text, binding kind)`
  tuples. Mapping text, binding text, and binding class are still asserted as a
  tuple per rendered row; nothing was weakened to a count or set check.
- Removed an exact duplicate of the base `.filter-toolbar` rules that had been
  copied inside the `max-width: 1120px` media query in `App.css`. Computed
  styles are unchanged; this is a narrow integration cleanup, not a refactor.
- No other source changes were made.

## Verification

All commands were run from the repository root. The Node path prefix used below
is `PATH="$HOME/.local/bin:$HOME/.local/share/node/node-v26.13.1-linux-x64/bin:$PATH"`.

| Check | Command | Result |
| --- | --- | --- |
| Typecheck | `npm --prefix frontend/docker-web-ui run typecheck` | passed |
| Production build | `npm --prefix frontend/docker-web-ui run build` | passed |
| Unit tests | `npm --prefix frontend/docker-web-ui run test:unit` | 9 passed |
| Focused filters browser suite | `frontend/docker-web-ui/tests/run_frontend_e2e.sh e2e/filters.spec.ts` | 4 passed, 7.2 s, run `3204632-18ec078e57bbdd65` |
| Repeated filters stress run | `frontend/docker-web-ui/tests/run_frontend_e2e.sh e2e/filters.spec.ts --repeat-each=3` | 12 passed, 20.8 s, run `3244708-18ec07c5154b567d` |
| Final full frontend runner | `frontend/docker-web-ui/tests/run_frontend_e2e.sh` | **23 passed (1.4 m)**, run `3256081-18ec07cd28fcdeda` |
| Mock overlap scenario | `OVERLAP_SCREENSHOT_PATH=/tmp/opencode/mvp_block6_overlap_mock_final.png npm --prefix frontend/docker-web-ui run test:e2e:mock` | 2 passed, 2.2 s |
| Live demo | `VITE_E2E=true DEMO_HOLD_SECONDS=900 frontend/docker-web-ui/tests/run_demo.sh` | demo run `3273941-18ec07e5f6255d89` |
| Capture | `node /tmp/opencode/mvp_block6_capture.cjs 3273941-18ec07e5f6255d89 http://localhost:35085 /tmp/opencode` | passed |

Logs:

- `/tmp/opencode/mvp_block6_focused_filters_e2e.log`
- `/tmp/opencode/mvp_block6_filters_repeat3.log`
- `/tmp/opencode/mvp_block6_full_frontend_e2e_final.log`
- `/tmp/opencode/mvp_block6_mock_overlap_final.log`
- `/tmp/opencode/mvp_block6_demo_final.log`
- `/tmp/opencode/mvp_block6_capture_final.log`
- `/tmp/opencode/mvp_block6_build.log`
- `/tmp/opencode/mvp_block6_unit.log`

The full browser runner used port `5173`, not production port `8080`, with a
real Rust backend and private DinD. The final full run was after the test-helper
and CSS cleanups.

## Live demo and exact-run fixture proof

Live UI during review: <http://localhost:5173/>. The demo was stopped after
capture and is not left running.

- Demo run ID: `3273941-18ec07e5f6255d89`.
- Backend: `http://localhost:35085`.
- Exact fixture prefix: `e2e-3273941-18ec07e5f6255d89-demo-`.
- API: 10 containers, 6 networks total, 11 edges.
- Fixture: 10 containers and 3 demo networks with exactly the expected names
  (`frontend`, `backend`, `data`, plus `demo-shared`).
- Rendered graph: 16 nodes and 11 edges.
- `demo-shared` is running on both the demo-frontend and demo-backend networks
  and has two Docker-reported current bindings:
  `127.0.0.1:<port> -> 8080/tcp` (loopback) and
  `0.0.0.0:<port> -> 8081/tcp` (wildcard).
- Network + published-binding filter result: 1 of 10 containers and 2 of 6
  networks visible, with the full stable frontend network ID shown.
- Overlap panel: `No overlaps`, 0 findings, across 4 evaluated networks.
- Console errors: none.
- Narrow metrics at 390 px: `innerWidth=390`, `clientWidth=390`,
  `scrollWidth=390`, `bodyScrollWidth=390`.

Evidence:

- [Live evidence JSON](/tmp/opencode/mvp_block6_live_evidence_3273941-18ec07e5f6255d89.json)
- [Topology snapshot](/tmp/opencode/mvp_block6_demo_topology_3273941-18ec07e5f6255d89.json)
- [Runtime config](/tmp/opencode/mvp_block6_demo_config_3273941-18ec07e5f6255d89.json)
- [Health response](/tmp/opencode/mvp_block6_demo_health_3273941-18ec07e5f6255d89.json)

## Screenshots reviewed

All were opened and reviewed after capture. Desktop used a 1440x900 viewport;
narrow used 390x844. Captures are full-page.

| Viewport / theme | State | Screenshot | SHA-256 |
| --- | --- | --- | --- |
| 1440x900 light | Default graph, filters, no-overlap panel | [PNG](/tmp/opencode/mvp_block6_desktop_light_default_3273941-18ec07e5f6255d89.png) | `120bdf24909d48d27e1825cf30955ace88f553ad6abe8092292ee7ec02b36dde` |
| 1440x900 light | `demo-shared` selected; dual membership and both mappings | [PNG](/tmp/opencode/mvp_block6_desktop_light_selected_shared_3273941-18ec07e5f6255d89.png) | `68a270c493600166c69be45685cf16aaffea359f9f11dd8bf0741cae3dd3f97e` |
| 1440x900 light | Frontend membership + published bindings filter | [PNG](/tmp/opencode/mvp_block6_desktop_light_filters_3273941-18ec07e5f6255d89.png) | `190417e0994fefe3fa9e9a23a3ea5b7dafc4a6f61950f675ce45386fa6d40780` |
| 1440x900 dark | `demo-shared` selected; dual membership and both mappings | [PNG](/tmp/opencode/mvp_block6_desktop_dark_selected_shared_3273941-18ec07e5f6255d89.png) | `2f238a7d5153e3d5ad7664cea7a5c0ecb485faa8bac6bee0a0330b59435e70f2` |
| 390x844 light | `demo-shared` selected; stacked layout | [PNG](/tmp/opencode/mvp_block6_narrow_390_light_selected_shared_3273941-18ec07e5f6255d89.png) | `8576964f895b04b714dad655e851a3e2dcd813fb3e6cdbe7c653087834f5976f` |
| 390x844 dark | `demo-shared` selected; stacked layout | [PNG](/tmp/opencode/mvp_block6_narrow_390_dark_selected_shared_3273941-18ec07e5f6255d89.png) | `33d257a5040455bf0f435bdea1373d3bafc65a65bc58d84e5c2f697abad65d1b` |
| 1280x720 light, mocked API | Rejected/impossible overlap finding, selected network, invalid coverage | [PNG](/tmp/opencode/mvp_block6_overlap_mock_final.png) | `eb7399c873a327c6e47801e17b51301fe59ceec511e8e7b2a30f0e8851151d67` |

## Visual findings

- No new visual regression was found in the BLOCK4 integration. The overlap
  panel sits cleanly between the filter toolbar and workspace in both themes.
- Desktop light and dark both show the dual-homed container, its two network
  attachments with full stable IDs, and loopback/wildcard bindings. The graph
  membership note remains visible.
- The filter state is legible at desktop and narrow widths, including the
  selected stable network ID and the visible/total counts.
- Narrow light and dark layouts have no horizontal overflow and keep search,
  filters, graph, list, details, and status usable.
- The live demo has distinct Docker CIDRs, so the overlap *finding* UI was
  reviewed with the mock-only impossible-overlap scenario. The mocked panel
  labels the problem as Docker-reported CIDR overlap, not traffic, and selecting
  a finding network clears filters and selects that network.
- Known non-blocking observation: selecting a container near the bottom of an
  internally scrollable list can leave a partially visible prior row at the top
  of that list. The same behavior exists in the block 3 baseline; it was not
  treated as a block 6 regression and was left unchanged to avoid a broad
  interaction refactor.
- Vite dev logs a non-error warning about Cytoscape custom wheel sensitivity;
  the browser console had no errors.

## Human pilots

Status: **one-user pilot has not yet been run**; **larger human pilot has not yet
been run**.

### 3-5 person short test script

Setup: one facilitator starts the demo with
`VITE_E2E=true DEMO_HOLD_SECONDS=900 frontend/docker-web-ui/tests/run_demo.sh`
and keeps it running. Use 3-5 Docker-familiar participants individually,
10-15 minutes each, with an observer recording completion, confusion, quotes,
and any blocker. Do not explain the UI unless the participant is stuck for more
than one minute.

Tasks and prompts:

1. Open <http://localhost:5173/>. Find the `demo-shared` container. Name the
   two networks it belongs to and the two host port mappings shown.
2. Explain in your own words what a graph edge means here. Does it prove the
   containers can communicate? Read the membership note aloud.
3. Filter to the demo-frontend network and enable published bindings. State how
   many containers and networks are visible, then clear the filters.
4. Switch to dark mode, reload the page, and confirm the theme persisted.
5. Resize the window to roughly 390 px wide. Confirm there is no horizontal
   scrolling and that details remain usable.
6. Open the subnet overview. Explain what "No overlaps" means for this demo.
   If available, repeat against the mock overlap build: open the finding, select
   a network, and confirm the search/filters clear and that network is shown.

Post-task questions (1-5, strongly disagree to strongly agree):

- I could identify the dual-homed container and both port mappings without help.
- I understand that edges mean Docker network membership, not traffic or
  reachability.
- The overlap wording made clear what was and was not being compared.
- I would trust this view for a quick troubleshooting check.

Success criteria: at least 4 of 5 participants complete tasks 1-3 without help,
all participants correctly explain the membership-only edge meaning, no P0/P1
usability blocker is found, and notes identify any wording or layout confusion.

Cleanup after the pilot: stop the demo with Ctrl+C or SIGTERM and confirm port
`5173` is free and no `docker-web-e2e` containers, images, or volumes remain.

## Cleanup and Git state

After the final demo and mock run:

- Port `5173`: no listener.
- Owned `docker-web-e2e` containers: none.
- Owned `docker-web-e2e` images: none.
- Owned `docker-web-e2e` volumes: none.
- No leftover demo, backend E2E, Vite preview, or capture processes.
- Generated `test-results-mocked/` was removed; screenshots and logs remain
  under `/tmp/opencode`.
- `git diff --check`: passed.
- No file was staged, committed, reset, or pushed by this block. Final
  production smoke remains for the other agent.
