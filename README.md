# Spotify CRO Playlist Creator

A Rust CLI tool that creates Spotify playlists from Radio Wave's daily programming. It fetches the Radio Wave playlist for a specified date and creates a corresponding Spotify playlist.

## Features

- Fetch Radio Wave playlist for a specific date
- Search for tracks on Spotify
- Automatic Spotify playlist creation
- OAuth2 authentication flow with Spotify
- Two modes in one binary: interactive CLI and a multi-user web app
- Web app (axum + askama) with per-visitor Spotify login and live per-track
  progress via Server-Sent Events
- Packaged as a multi-stage Docker image for Google Cloud Run (rustls TLS)

## Installation

1. Clone the repository:
```bash
git clone https://github.com/VojtaStanek/spotify_cro_playlist_creator.git
cd spotify_cro_playlist_creator
```

2. Build the project:
```bash
cargo build --release
```

## Configuration

1. Use following environment variables to configure the tool:
```
RSPOTIFY_CLIENT_ID=your_client_id
RSPOTIFY_CLIENT_SECRET=your_client_secret
RSPOTIFY_REDIRECT_URI=http://127.0.0.1:8888/callback
RUST_LOG=info
# 64+ random bytes; used to encrypt the session cookie (web mode).
SESSION_SECRET=change-me-to-64-or-more-random-bytes-................................
```

Generate a strong `SESSION_SECRET`, for example:

```bash
openssl rand -base64 64
```

If `SESSION_SECRET` is unset, the web server generates a random ephemeral key at
startup and logs a warning; sessions then don't survive restarts or span multiple
instances.

2. Ensure you have registered your application in the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) and added `http://127.0.0.1:8888/callback` as a redirect URI.

The app loads a local `.env` file automatically (via `dotenvy`) when present, so
you can put the variables above in `.env` for local development.

## Usage

The single binary runs in two modes, chosen automatically:

- **CLI mode** — when the only argument is a `YYYY-MM-DD` date.
- **Web server mode** — when there is no such date argument.

### CLI mode

Run the tool by providing a date in YYYY-MM-DD format:

```bash
spotify_cro_playlist_creator 2024-09-01
```

The tool will:
1. Open your browser for Spotify authentication (interactive `prompt_for_token`)
2. Fetch the Radio Wave playlist for the specified date
3. Search for matching tracks on Spotify
4. Create a new playlist titled "Radio Wave YYYY-MM-DD" - eg. "Radio Wave 2024-09-01"
5. Add all found tracks to the playlist

### Web server mode

Run the binary with no date argument to start the web server:

```bash
cargo run
# or, after building: ./target/release/spotify_cro_playlist_creator
```

The server binds `0.0.0.0:$PORT` (default `8080`). Each visitor logs in with their
own Spotify account and playlists are created in their library. Per-track progress
is streamed live to the browser via Server-Sent Events.

Session state (the visitor's Spotify token) is stored entirely client-side in a
single AEAD-encrypted cookie (`axum-extra`'s `PrivateCookieJar`), encrypted with a
key derived from `SESSION_SECRET`. There is no server-side session store, so the app
is stateless and scales horizontally.

Routes: `GET /` (login / date form), `GET /login`, `GET /callback`, `GET /create`,
`GET /create/stream` (SSE), `GET /logout`, `GET /healthz`.

**Local web testing:** the OAuth redirect URI's port must match the port the server
binds. The provided `.env` uses `RSPOTIFY_REDIRECT_URI=http://127.0.0.1:8888/callback`,
so start the server on port 8888 and open <http://127.0.0.1:8888/>:

```bash
PORT=8888 cargo run
```

Make sure `http://127.0.0.1:8888/callback` is registered as a Redirect URI in the
Spotify Developer Dashboard.

## Docker

The multi-stage `Dockerfile` builds a release binary and ships it on
`debian:bookworm-slim` with only `ca-certificates` (TLS uses rustls, so no OpenSSL
is required). Templates are embedded into the binary at compile time.

```bash
docker build -t cro-web .
docker run -p 8080:8080 --env-file .env cro-web
```

Then hit <http://localhost:8080/healthz> (returns `200 ok`) and use the web flow.
Note the redirect-URI/port constraint above also applies inside Docker.

## Google Cloud Run deployment

- The service listens on `0.0.0.0:$PORT`; Cloud Run sets `PORT` automatically.
- Set `RSPOTIFY_CLIENT_ID` and `RSPOTIFY_CLIENT_SECRET` as Cloud Run secrets / env vars.
- Set `RSPOTIFY_REDIRECT_URI=https://<service-url>/callback` and add that exact URL
  as a Redirect URI in the Spotify Developer Dashboard (keep the localhost one for
  CLI / local dev).
- Raise the request timeout (~600s) so long SSE streams are not cut off.
- Set `SESSION_SECRET` (64+ random bytes, e.g. `openssl rand -base64 64`) as a Cloud
  Run secret. Because sessions live entirely in an encrypted cookie keyed on this
  secret, logins survive restarts and work across any number of instances as long as
  every instance shares the same `SESSION_SECRET`. You can scale freely — the previous
  `min/max-instances=1` requirement no longer applies.

## API Reference

The tool uses the Radio Wave API endpoint:
```
https://api.rozhlas.cz/data/v2/playlist/day/{year}/{month}/{day}/radiowave.json
```

## License

MIT
