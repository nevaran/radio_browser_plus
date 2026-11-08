# Radio Browser Plus Guide

## Project Layout

### Directory Structure
* `src/` Rust Axum backend (`radio_browser_plus` binary): JSON API + serves `dist/`
* `src/main.rs` entrypoint: tracing init, env config, service wiring, router and middleware assembly
* `src/lib.rs` library exports
* `src/domain.rs` serde models for the upstream radio-browser.info API and the app's JSON responses
* `src/error.rs` `AppError` (thiserror) and `Result<T>`; `IntoResponse` maps variants to status + `{"error": msg}`
* `src/auth.rs` (tests in sibling `src/auth/tests.rs`) `AuthService`: users, sessions, guest sessions, bcrypt, cookie auth, presence tracking, activity log
* `src/features/` one handler service per area: `stations.rs`, `collections.rs`, `favorites.rs`, `health.rs`
* `src/infra/` stateful adapters: `radio_browser_client.rs` (reqwest to the upstream API), `favorites_repository.rs` (per-user JSON persistence)
* `src/genres.rs` curated genre catalog (`CanonicalGenre`, `VARIETY_GENRE` catch-all)
* `frontend/` Leptos 0.8 CSR WebAssembly SPA (`radio-browser-plus-frontend`, wasm-only)
  * `frontend/src/app.rs` root component, keyboard shortcuts, heartbeat interval
  * `frontend/src/state.rs` all shared reactive state (`RwSignal`) plus data loading: generation counters, retries, debounce, localStorage
  * `frontend/src/api.rs` typed `gloo-net` client for `/api/*` and `/health`
  * `frontend/src/models.rs` response/request types mirrored from `src/domain.rs`
  * `frontend/src/player.rs` `HtmlAudioElement` streaming, volume, reconnect
  * `frontend/src/components/` sidebar, grids, now playing, modals, dialogs
  * `frontend/src/utils.rs` sorting, name normalization, percent-encoding
  * `frontend/Trunk.toml` dev server config: proxies `/api` and `/health` to port 8000
* `dist/` Trunk build output served by the backend (gitignored)
* `data/` runtime state (accounts, per-user favorites, optional activity log) — never edit or commit
* `Cargo.toml` (root) workspace: `members = ["frontend"]`, `default-members = ["."]`

### Key Components
- **API surface**: `/api/*` routes are grouped per service in `src/main.rs` (`station_routes`, `collection_routes`, `auth_routes`, `favorite_routes`, `config_routes`). Everything under `/api` sits behind the `require_auth` middleware; public paths are allowlisted in `is_public_api_path`. `/health` stays in its own public router so probes need no credentials.
- **Auth**: `AuthService` keeps users/sessions/guest sessions in memory (`std::sync` locks) with JSON persistence in the data dir. Cookie sessions are `HttpOnly; SameSite=Lax`, `Secure` added when the request arrived over HTTPS, 24h Max-Age renewed on login. Passwords are bcrypt-hashed; changing a password revokes sessions. Unknown `/api` paths return JSON 404, never the SPA shell.
- **Upstream client**: `RadioBrowserClient` wraps a `reqwest::Client` pointed at a public radio-browser.info mirror; responses are body-capped and per-request failures map to `AppError::ExternalServiceError` (502).
- **Curated genres**: `src/genres.rs` maps raw upstream tags (synonym sets, matched case-insensitively, disjoint per test) to canonical genre names. Genre queries fan out over their tags with `futures_util::future::join_all` and merge round-robin so every tag contributes proportionally — deliberately no popularity sort, that happens client-side.
- **Favorites**: one directory per user (`data/user-<name>/favorites.json`, 0700 dir / 0600 file), an in-memory `tokio::sync::RwLock` cache, and atomic writes (temp file + rename, used everywhere persistence happens).

## Rust Rules

* Avoid `unwrap()` / `expect()` in production code.
* Exceptions used in this codebase:

  * tests;
  * lock acquisition where failure means poisoning, not normal control flow (e.g. `AuthService`'s `std` locks);
  * infallible env/default fallbacks written as `unwrap_or_else(...)` / `unwrap_or_default()`, and format-string-only `expect`s that cannot fail (`cookie` header assembly).
* Otherwise prefer the `error::Result<T>` alias + `?` or explicit handling. `AppError` messages are user-visible JSON error bodies — word them accordingly.
* Do not ignore errors silently. Log with `tracing::warn!`/`error!` when a failure can't be propagated back to the caller; `let _ = ...` only for best-effort cleanup (e.g. removing a temp file).
* Avoid unnecessary `.clone()`. Prefer borrows; clone at state boundaries (per-user handlers own their inputs).
* Do not add dependencies unless needed. The web stack (axum 0.8 / tokio 1 / reqwest with rustls) and bcrypt are the anchors; check the existing graph before reaching for a new crate.
* Keep code simple and idiomatic.

### Logging

* Backend logging is `tracing` with `EnvFilter` (reads `RUST_LOG`, defaults to `info`) writing to stdout: what appears in container logs is exactly what the level allows.
* Request-flow and per-item logs (one per request, per upstream fetch, per row) use `debug!`. Never raise routine high-frequency events to `info` or `warn` — at the default level they flood a container's stdout.
* `info!` is for startup and major lifecycle lines (listening, data-dir init, config flags); `warn!`/`error!` are for faults worth seeing in a user's log.
* Never log secrets: passwords, password hashes, session ids, or cookie values. Username/IP/station presence data is intentionally recorded — through the gated activity log, not log lines.

## Tokio Rules

* Assume a Tokio runtime already exists: exactly one, the `#[tokio::main]` multi-thread runtime in `src/main.rs`.
* Never create nested runtimes, or a second runtime for background helpers — spawn on the shared one.
* Never call `Runtime::block_on()` inside Tokio / async code.
* Do not hide runtime creation inside helpers or libraries. Handlers and services take the existing runtime for granted; nothing in the workspace builds its own.
* Do not hold locks across `.await`. Both lock kinds here follow that:

  * `std::sync::{Mutex, RwLock}` (AuthService, presence tracker): guards are taken, used, and dropped before any await — collect what you need first;
  * `tokio::sync::RwLock` (favorites cache): same rule, no guard returned from a function.
* Prefer `.await`, `tokio::spawn`, channels; concurrent upstream fetches use `futures_util::future::join_all` (see the genre fan-out).
* Blocking work: `AuthService` is deliberately synchronous (std locks + `std::fs` + bcrypt) and called from async handlers — its blocking sections are short. New work that is heavy (long scans, bulk hashing, large file IO) belongs in `spawn_blocking` or a dedicated thread; grow new heavyweight work the async-safe way, and use `tokio::fs` for new file IO outside the auth service.
* Do not use `std::thread::sleep()` in async code. The backend uses the `TimeoutLayer`; the frontend uses gloo timers.

## Leptos Frontend

* The frontend crate is wasm-only: default cargo commands target the backend. Build it explicitly with `cd frontend && trunk build --release` (output: root `dist/`, artifacts: root `target/`), or serve with `trunk serve` (dev port 8080, proxies `/api` and `/health` to the backend on 8000).
* Do not change `[profile.release.package.radio-browser-plus-frontend] strip = false` in the root `Cargo.toml`: `wasm-opt` needs the emitted `target_features` section to enable its bulk-memory/nontrapping ops (see the comment there).
* All HTTP goes through `frontend/src/api.rs`. It always sets same-origin credentials so the session cookie flows, schedules through typed request/response types in `models.rs`, and returns the literal string `"unauthorized"` on 401/403 — state code keys off that to open the sign-in modal. Keep the convention.
* Adding an endpoint end to end:

  1. model in `src/domain.rs` (`#[serde(default)]` on lenient fields);
  2. handler in the owning `src/features/` service, with input validation like the existing length caps / per-user caps;
  3. route in `src/main.rs` (it inherits the auth middleware by merging into the `api` router; only extend `is_public_api_path` — with its test — for genuinely public endpoints);
  4. response type mirrored in `frontend/src/models.rs`, typed call in `frontend/src/api.rs`;
  5. consumed from `state.rs` or a component via `leptos::task::spawn_local`.
* Async in components runs on `leptos::task::spawn_local`. Stale-response discarding uses generation counters, not `AbortController` (see the `state.rs` header comment).
* All shared reactive state lives in `state.rs` as `RwSignal` fields on `AppState`; components take slices of it. Do not invent module-level mutable globals in components.
* Grids render incrementally (infinite scroll, `GRID_PAGE` page size) because the backend can return thousands of stations; do not render unbounded lists.
* In-app confirmation/alerts go through the `AppDialog` signal and `Dialog` component — do not reintroduce browser-native `alert()`/`confirm()`.
* Browser access goes through `web-sys`/`js-sys` bindings already gated by the feature list in `frontend/Cargo.toml`; add the specific feature, not `web-sys`'s wide feature gaps.

## Editing Hygiene

* Change only what is required.
* Prefer the smallest valid diff.
* Do not refactor unrelated code.
* Do not make formatting-only changes.
* Keep naming/style consistent with nearby code.

### Comments

* Avoid comments unless they explain a non-obvious reason, constraint, or workaround.
* This codebase's comments document *why* (why the CSP lists `'unsafe-inline'`, why the genre merge is round-robin and unsorted, why writes are atomic, why a cap exists). Never restate what the code does; prefer clearer code instead.
* If the code is self-explanatory, add no comment.

### Imports

* Match the file's existing import shape and order: `std` first (flat or braced, whichever the file uses), then external crates, then `use crate::` items.
* Group what belongs together while staying within the file's style; do not restyle existing import blocks.
* A `#[cfg(...)]` import must stand on its own — an attribute binds to one item.

### Be minimally invasive

* Prefer purely additive changes: new functions/consts around existing code instead of restructuring it. The ideal diff for a fix adds lines and modifies/deletes none.
* Do not extract or reshape existing code just to enable new code. Accept a little duplication over a restructure.
* Put new logic in the module that owns the concern; call sites in shared files (`src/main.rs`, `state.rs`) stay thin.
* Before changing a widely used signature (`AppError`, `AuthService`, `domain.rs` models, `State` extractors), check whether the need is specific to one path. If it is, keep the change there unless impossible.
* If an unrelated caller needs placeholder arguments solely to satisfy a changed signature, the diff is too broad: stop and redesign.
* Do not fix unrelated pre-existing issues in the same change unless they directly block it.

### Mandatory regression-surface check

Before considering any implementation complete, perform a minimization pass over the final diff.

* Inspect every modified existing file and path. Each must be strictly necessary for the requested change; revert cleanup-only or consistency-only changes.
* Keep new implementation logic in new or feature-specific modules; changes to `main.rs` / shared services should be thin hooks (a route, a state field), not rewrites.
* Before finalizing, explicitly report the regression surface: which existing runtime paths changed behavior and why each change is unavoidable.

## Configuration and Deployment

* Env vars (all documented in `.env.example` and its `docker-compose.yml.example` mapping): `RADIO_BROWSER_STATION_LIMIT`, `ALLOW_GUEST_ACCESS`, `ADMIN_STATS_ENABLED`, `LOG_RETENTION_DAYS`, plus `UID`/`GID` and `DATA_DIR` (non-docker / local runs).
* `0`/unset means default; boolean flags accept `1/true/yes/on` case-insensitively via `parse_guest_flag`. New env parsing goes through small named parse functions in the owning file so tests can pass values instead of touching process env.
* Adding an env var means touching, and keeping consistent: the parse function (+ tests), `.env.example`, `docker-compose.yml.example` (the plain `docker-compose.yml` is a local gitignored copy), and the README configuration table.
* Deployment shape: multi-stage Docker build (Trunk frontend build, then a musl static backend binary), non-root user, read-only root filesystem, port 8000, `data/` mounted at `/app/data`. Runs behind an external reverse proxy on the `proxy-frontend` network; HTTPS terminates there.
* `.env`, `docker-compose.yml`, `target/`, and `dist/` are local artifacts; never commit them. Same for anything under `data/` (it holds password hashes).

## Tests

* `cargo test` from the repo root runs the backend suite (the workspace's `default-members` keeps it that way). `cargo test --workspace` additionally compiles and runs the frontend crate's host-compatible tests (`state.rs`, `utils.rs`).
* Tests live beside the code in a `#[cfg(test)] mod tests`. The one structural exception: `AuthService` tests live in the sibling file `src/auth/tests.rs`, included by `#[cfg(test)] mod tests;` at the end of `src/auth.rs`; use that split only when a module outgrows its file.
* Assert observable behavior of pure helpers (parse functions, limits, merge logic, public-path checks). Do not test private state or every corner raised in a review.
* Tests must not depend on ambient process env: pass values through the parse helpers' parameters.
* `AuthService` tests run against a fresh temp dir (`std::env::temp_dir()` + a uuid suffix) and clean up afterwards.
* No test harnesses, runners, or new dev-dependencies; the suite is plain unit tests by design.

## Build and Run

* Requirements: Rust 1.98+, the `wasm32-unknown-unknown` target, and Trunk ^0.22.
* Dev loop: `cargo run` (backend, port 8000) + `cd frontend && trunk serve` (UI on 8080 with API proxy).
* Release build: `cd frontend && trunk build --release` then `cargo build --release` (uses `--locked` in Docker; keep `Cargo.lock` accurate).
* Docker: `docker compose up -d --build` (compose network must be `proxy-frontend`).
