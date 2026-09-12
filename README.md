# Radio Browser Plus

A standalone web application for discovering, streaming, and organizing radio stations from the [Radio Browser API](https://www.radio-browser.info/).

Radio Browser Plus was initially taken from the Home Assistant Radio Browser integration and then developed and improved into a standalone app and migrated to Rust for performance and security improvements. It keeps the useful radio-station discovery experience while adding its own web interface, authentication, favorites, collections, and persistent data storage.

Both the backend and the frontend are written in Rust: the backend is an
Axum JSON API, and the browser UI is a [Leptos](https://leptos.dev/) WebAssembly
single-page app (client-side rendering) that talks to that API.

## Features

- Browse radio stations by:
  - **All Stations**: Every station fetched in a large list
  - **Popular**: Most clicked stations
  - **Favorites**: Browse only favorited radios
  - **Countries**: Find stations by country of origin
  - **Languages**: Filter by language using certain tags
  - **Genre**: Discover stations using certain tags
- Search functionality to find specific radio stations
- Stream audio through any device that supports a web browser
- User authentication and per-user favorites
- Persistent favorites stored in the local `data/` directory

## Installation

### Docker

The container stores application data in the local `data/` directory. Start it
with Docker Compose:

```bash
docker compose build && docker compose up -d
```

The Compose volume maps `./data` to `/app/data`, so favorites and user data
survive container recreation. The application is available at
`http://localhost:8000`.

The container runs as a non-root user and does not require Linux capabilities for its web server, file access, or outbound API requests thus the use of
```yaml
cap_drop:
  - ALL
```

`RADIO_BROWSER_STATION_LIMIT` controls the default number of stations fetched
per request and defaults to `1000`. Set it in `.env` to change the default
(`0` behaves like the default). Explicit `?limit=` values are capped at
10,000 stations per request to bound memory use.

### From source

Requirements:

- Rust 1.98+
- Cargo
- For frontend work: the `wasm32-unknown-unknown` target and
  Trunk(`rustup target add wasm32-unknown-unknown`
  and `cargo install trunk --locked`)

The repo is a Cargo workspace (backend + `frontend/` members) with a single
`Cargo.lock` and a single `target/` directory at the root; Trunk also writes
its output to the root `dist/`. Plain `cargo build` / `cargo test` only target
the backend — the wasm-only frontend is built explicitly via Trunk.

Build the frontend once (output goes to `dist/`, served by the backend):

```bash
cd frontend && trunk build --release && cd ..
```

Run the application with:

```bash
cargo run --release
```

The server listens on port `8000` by default and serves the API plus the
built frontend from `dist/`.

### Frontend development

The Leptos app lives in `frontend/` and calls the same `/api/*` endpoints as
the previous JavaScript UI — the backend API is unchanged. For UI work, run
the backend and the Trunk dev server (which proxies `/api` and `/health` to
port 8000) side by side:

```bash
cargo run
cd frontend && trunk serve
```

Then open the URL printed by Trunk (port 8080 by default).

## First Configuration
1. Open `http://localhost:8000` in a browser
2. Sign in with the application user account. Default first user is admin/admin
3. Change the admin password immediately (Account → Change Password). The
   server logs a warning on startup while the default password is in use.
4. Browse or search for radio stations, create more users
5. Select a station to stream it

All API endpoints except sign-in/sign-out require authentication, and the web
UI blocks usage behind the sign-in dialog until you are logged in.

## Usage

After signing in, browse popular stations, search the Radio Browser catalog,
filter stations by country, language, or tags, and select a station to stream.

### Features

- ⭐ Favorites appear first in the media browser
- Favorited stations are marked with a yellow star (⭐)
- Favorites are stored per Home Assistant config entry
- Favorites persist across restarts
- Native Home Assistant entities for favorites management
- Separate favorites list for each Radio Browser Plus integration

## Frontend architecture (Leptos)

- `frontend/src/` — components (`sidebar`, `station_grid`, `collection_grid`,
  `now_playing`, `auth_modals`), typed API client (`api.rs`), reactive state
  (`state.rs`), audio playback (`player.rs`), data models (`models.rs`), pure
  helpers (`utils.rs`, covered by unit tests).
- `frontend/index.html` — Trunk entry shell; Trunk injects the WASM bundle,
  the (unchanged) stylesheet, the favicon, and the flag icons (`/flags/*`).
- The Axum backend serves the root `dist/` (see `Dockerfile`, which builds the
  frontend with Trunk and then the backend as a static binary).

Notes on the migration from the previous JavaScript UI (`public/app.js`,
`public/sort-worker.js`, removed):

- Same views (All, Popular, Favorites, Countries, Languages, Genres), search
  with debounce, `#view/filter` hash deep links, auth dialogs, favorites with
  live metadata refresh, now-playing bar with stream-health indicator, volume /
  mute with persistence, and keyboard shortcuts (`M`, `Space`, arrows).
- Performance: fine-grained reactivity re-renders only changed nodes; sorting
  runs directly in WASM (the sort Web Worker was removed as obsolete); station
  artwork uses native lazy loading.
- One intentional behavior fix: the old retry chain cleared its "is playing"
  flag before checking it, so it could never run. The Rust player tracks
  playback intent explicitly, so the 5 × 3s retry chain now works.

## Support

For issues, feature requests, or questions, please visit the [GitHub repository](https://github.com/nevaran/radio_browser_plus/issues).

## License

This project is licensed under a Personal and Non-Commercial Use License - see the LICENSE file for details.

## Credits

The project was initially based on the [Home Assistant Radio Browser integration](https://www.home-assistant.io/integrations/radio_browser/), then made and improved into a standalone application.

Radio station data provided by [Radio Browser](https://www.radio-browser.info/).
