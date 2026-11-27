# MVP UI review (block 3)

Reviewed 2026-11-27 on `feature/frontend-backend-e2e`. Worktree changes were
left unstaged; no commits were made.

## Verification

| Check | Result |
| --- | --- |
| `npm --prefix frontend/docker-web-ui run typecheck` | passed |
| `npm --prefix frontend/docker-web-ui run build:e2e` | passed |
| `npm --prefix frontend/docker-web-ui run build` | passed |
| `frontend/docker-web-ui/tests/run_frontend_e2e.sh` | **22 passed (1.4m)**, exit 0, run `2997771-18ebcd6c53c5cb22` |
| `backend/tests/run_backend_e2e.sh` | **39 passed; 0 failed; 2 ignored** in 209.42 s, exit 0 |
| `cargo test --manifest-path backend/Cargo.toml --lib` | **13 passed** |
| `git diff --check` | passed |

Logs:

- `/tmp/opencode/mvp_block3_full_browser_e2e_final2.log`
- `/tmp/opencode/mvp_block3_full_backend_e2e_final.log`

## Live demo review

Started separately with `DEMO_HOLD_SECONDS=900 frontend/docker-web-ui/tests/run_demo.sh`.
The readiness token was `2918002-18ebccc79d118244`; the exact fixture prefix was
`e2e-2918002-18ebccc79d118244-demo-`. The backend returned 10 containers,
6 networks (3 fixture networks plus `bridge`/`host`/`none`), and 11 edges. The
multi-homed `demo-shared` container belonged to both `demo-frontend` and
`demo-backend`, with Docker-reported current bindings
`127.0.0.1:<port> -> 8080/tcp` (loopback) and
`0.0.0.0:<port> -> 8081/tcp` (wildcard).

Evidence JSON:
`/tmp/opencode/mvp_block3_live_evidence_2918002-18ebccc79d118244.json`.

Screenshots (opened and reviewed):

- `/tmp/opencode/mvp_block3_desktop_light_default_2918002-18ebccc79d118244.png`
- `/tmp/opencode/mvp_block3_desktop_light_selected_shared_2918002-18ebccc79d118244.png`
- `/tmp/opencode/mvp_block3_desktop_light_filters_published_2918002-18ebccc79d118244.png`
- `/tmp/opencode/mvp_block3_desktop_dark_selected_shared_2918002-18ebccc79d118244.png`
- `/tmp/opencode/mvp_block3_narrow_390_light_selected_shared_2918002-18ebccc79d118244.png`
- `/tmp/opencode/mvp_block3_narrow_390_dark_selected_shared_2918002-18ebccc79d118244.png`

## Visual findings and fixes

- Light-theme graph labels were white on a white canvas. Labels now use dark
  text with a light outline in light mode and remain white in dark mode.
- Full generated fixture names made graph labels unreadable at desktop and
  narrow widths. Graph labels now use a compact suffix for long names; lists and
  details keep full Docker names and IDs.
- Network filtering and published binding controls moved into a dedicated
  toolbar so the header no longer wraps the title into three lines.
- The selected network filter shows the full stable Docker network ID below the
  select control, in addition to the ID-valued option.
- Selection and details for networks come from the full topology snapshot; a
  selected empty network is pinned into the filtered graph and remains
  selectable. No edges are synthesized for filters.
- Dark mode was rechecked after transitions settled; previous low-contrast list
  items were a capture-timing artifact, not a CSS defect.
- Narrow 390 px light and dark layouts have no horizontal overflow
  (`innerWidth/clientWidth/scrollWidth = 390`), keep filters, graph, list, and
  details usable, and retain the selected membership/binding details.

## Overlap-panel integration points

- Stable identity: use `Network.id` and attachment `network_id` for filtering,
  selection, and future subnet overlap grouping. Names are display-only.
- CIDRs: `Network.ipam_configs` preserves Docker's complete ordered IPAM list;
  `Network.subnet` remains the legacy first subnet. `getNetworkIpamConfigs()`
  normalizes both for rendering.
- Membership truth: use supplied `topology.edges` only. Helpers
  `containerIsMemberOfNetwork`, `getNetworkMembersWithAttachments`, and
  `getContainerNetworkAttachments` expose edge-derived membership plus
  per-endpoint IPv4/IPv6/MAC metadata without inventing traffic edges.
- Ports: `classifyHostBinding`, `currentPublishedBindings`, and
  `formatPortBinding` provide exact `unknown`/`wildcard`/`loopback`/`specific`
  labels and IPv6 bracket formatting. Only running/paused rows with non-null
  `public` count as Docker-reported current host bindings.
- Filter/selection: `filterTopology` accepts `networkId`, `publishedOnly`, and a
  pinned selected network ID, and composes them with the existing text query.
  It returns the full totals plus the filtered induced edge set for counts and
  summaries.
- Inactive containers keep retained Docker metadata but are explicitly marked
  as not current graph membership and not current published bindings.
