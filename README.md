# Radio Browser Plus

A self-hosted web app for discovering, streaming, and organizing stations from the [Radio Browser API](https://www.radio-browser.info/). The backend is an Axum JSON API; the browser client is a Rust and [Leptos](https://leptos.dev/) WebAssembly single-page app.

## Features

- Browse all or popular stations, and explore by country, language, or genre.
- Search and sort by name, popularity, or bitrate.
- Stream stations with playback controls and resilient reconnect behavior.
- Create accounts and save per-user favorites.
- Optionally allow guest browsing.
- Admins can view active users and guests, including their current station, IP address, and active time.
- Optionally record account, session, and station activity in a retained text log.
- Incremental station grids, retry states, and mobile-friendly layouts.

Keyboard shortcuts: `/` focuses search, `Space` toggles playback, `M` toggles mute, and the arrow keys adjust volume.

## Docker Deployment

1. Copy `.env.example` to `.env` and set the options described below.
2. Ensure the external Docker network `proxy-frontend` exists:

   ```bash
   docker network create proxy-frontend
   ```

   Skip this if the network already exists.
3. Build and start the app:

   ```bash
   docker compose up -d --build
   ```

Compose attaches the app to `proxy-frontend` and does not publish port 8000 on the host. Configure your reverse proxy on that network to forward to `radio-browser-plus:8000` and terminate HTTPS there. For direct local access, uncomment the port mapping in `docker-compose.yml`.

The `./data` directory is mounted at `/app/data` and persists users, sessions, favorites, and the optional activity log. The container runs as a non-root user, drops Linux capabilities, and uses a read-only root filesystem.

## Configuration

Set these variables in `.env`; Compose forwards them to the container.

| Variable | Default | Description |
| --- | --- | --- |
| `RADIO_BROWSER_STATION_LIMIT` | `1000` | Default station request limit. `0` uses the default. |
| `ALLOW_GUEST_ACCESS` | `0` | Set to `1`, `true`, `yes`, or `on` to allow browsing without signing in. Favorites and account actions still require an account. |
| `ADMIN_STATS_ENABLED` | `0` | Enable the admin-only active sessions view and presence tracking. |
| `LOG_RETENTION_DAYS` | `0` | Keep activity entries for this many days. `0` disables activity logging. When enabled, the readable log is written to `/app/data/activity.log`. |

The active sessions view reports clients that send periodic page heartbeats; entries expire after the heartbeat timeout. The activity log can contain usernames, IP addresses, and station names. Keep the data directory private and do not expose the log through the web server.

### First Login

On an empty data directory, the server creates the default account `admin` / `admin`. Sign in and change that password immediately before exposing the service. The server logs a warning while the default password remains in use.

## Run from Source

Requirements: Rust 1.98 or newer, Cargo, the `wasm32-unknown-unknown` target, and [Trunk](https://trunkrs.dev/).

Install the frontend toolchain if needed:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --version ^0.22 --locked
```

Build the frontend and start the backend from the repository root:

```bash
cd frontend && trunk build --release && cd ..
cargo run --release
```

The server listens on port 8000 and serves the API and built frontend from `dist/`. The Cargo workspace shares one `Cargo.lock` and one root `target/` directory.

### Frontend Development

Run the backend in one terminal and Trunk in another:

```bash
cargo run
```

```bash
cd frontend && trunk serve
```

Trunk serves the frontend (port 8080 by default) and proxies `/api` and `/health` to the backend on port 8000.

## Support and Credits

For issues and feature requests, use the [GitHub issue tracker](https://github.com/nevaran/radio_browser_plus/issues). Station data is provided by [Radio Browser](https://www.radio-browser.info/).

## License

Personal and non-commercial use only. See [LICENSE](LICENSE) for the full terms.
