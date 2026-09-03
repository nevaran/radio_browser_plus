# Radio Browser Plus

A standalone web application for discovering, streaming, and organizing radio stations from the [Radio Browser API](https://www.radio-browser.info/).

Radio Browser Plus was initially taken from the Home Assistant Radio Browser integration and then developed and improved into a standalone app. It keeps the useful radio-station discovery experience while adding its own web interface, authentication, favorites, collections, and persistent data storage.

## Features

- Browse radio stations by:
  - **Popular**: Most clicked stations
  - **Category**: Browse by music genres and categories
  - **Language**: Filter by language
  - **Country**: Find stations by country
  - **Genre**: Discover stations using certain tags
- Search functionality to find specific radio stations
- Stream audio through Home Assistant media players
- Click tracking (registers your listening with Radio Browser)
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
per request and defaults to `1000`. Set it in `.env` to change the default. Set
it to `0` to omit the API limit and fetch all available stations; this can use
significantly more time, memory, and bandwidth.

### From source

Requirements:

- Rust 1.94+
- Cargo

Run the application with:

```bash
cargo run --release
```

The server listens on port `8000` by default.

## First Configuration

1. Open `http://localhost:8000` in a browser
2. Sign in with the application user account. Default first user is admin/admin
3. Browse or search for radio stations, create more users
4. Select a station to stream it

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

## Support

For issues, feature requests, or questions, please visit the [GitHub repository](https://github.com/yourusername/radio_browser_plus/issues).

## License

This project is licensed under a Personal and Non-Commercial Use License - see the LICENSE file for details.

## Credits

The project was initially based on the [Home Assistant Radio Browser integration](https://www.home-assistant.io/integrations/radio_browser/), then made and improved into a standalone application.

Radio station data provided by [Radio Browser](https://www.radio-browser.info/).
