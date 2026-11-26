# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project Overview

Docker Web is a read-only viewer for Docker network topology: networks, attached
containers, IP addresses, and published ports. The backend talks to the Docker
Engine API; the frontend renders the topology graph.

- `backend/` — Rust service (Actix Web + bollard), edition 2024.
- `frontend/docker-web-ui/` — React + TypeScript + Vite UI.
- `config/` — application configuration.
- Do not hardcode configuration. Read it from environment variables and keep
  local values in a `.env` file.

## Testing

Testing effort should be weighted toward end-to-end tests, not unit tests. Prefer
a small number of realistic E2E tests over a large suite of isolated unit tests.

- **Prioritize E2E/integration tests** that exercise real functionality. Add unit
  tests only for pure logic that is impractical to cover end to end.
- **Prefer testcontainers over unit tests.** Bring up real containers and
  networks so tests run against real Docker API responses.
- **Never test against the Docker environment on the host machine.** Use
  testcontainers to create a controlled, isolated environment.
- Do not mock a pile of collaborators and then assert that the mock works; test
  real behavior across real boundaries.
- Name tests `method_name_state_under_test_expected_behavior` in snake_case.
- Structure every test with explicit GIVEN / WHEN / THEN sections.
- Remove redundant tests; do not duplicate coverage.
- Backend tests live in `backend/tests/`; frontend E2E tests use Playwright.

## Rust

- `backend/` is the Rust project. Add dependencies with `cargo add <crate>`;
  do not hand-edit dependency versions when `cargo add` can do it.
- Keep code clean under `cargo fmt` and `cargo clippy --all-targets -- -D warnings`.
- Prefer returning `Result` with `thiserror` errors; avoid `unwrap`, `expect`,
  and `panic!` in production paths.
- Avoid blocking work inside async tasks; use the async `bollard` and `tokio`
  APIs.
- Actix Web handlers should return `Result` and map errors to appropriate HTTP
  status codes without leaking internals.
- Centralize configuration in the `config` module and read it from the
  environment; provide defaults only where safe.
- Use `serde` DTOs at API boundaries and `validator` for input validation.
- Use the `log` crate instead of `println!`.
- Document public items with `///` doc comments.
- Keep modules focused (`api`, `config`, `docker`); avoid oversized functions and
  unnecessary `.clone()` calls.
- Do not introduce `unsafe` without a strong, documented reason.

## Frontend

- Use TypeScript in strict mode; avoid `any` and non-null assertions.
- Use React 19 function components and hooks; no class components.
- Run `npm run typecheck` and `npm run build` before finishing changes.
- Keep API access in `src/services/api.ts` and shared types in `src/types.ts`;
  do not scatter `fetch` calls or duplicate DTO shapes.
- Put components in `src/components/` with colocated CSS, following existing
  naming and styling patterns.
- Prefer local state; lift or share state only when necessary. Handle loading,
  empty, and error states explicitly.
- Write semantic, accessible markup (labels, roles, keyboard navigation, focus
  states); the UI must work in both themes.
- Configure the UI through Vite `VITE_*` environment variables; never hardcode
  endpoints or secrets.
- Clean up Cytoscape instances, listeners, and effects on unmount; memoize
  expensive topology computations.
- Prefer Playwright E2E tests for user flows, using role- or `data-testid`-based
  selectors.

## Workflow

- Keep changes focused and commits small; use conventional commit prefixes
  (`feat:`, `fix:`, `test:`, `docs:`, `refactor:`).
- Do not commit secrets or credentials.
- Run formatting, linting, and type checks before opening a pull request.
