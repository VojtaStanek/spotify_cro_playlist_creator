# Spotify CRO Playlist Creator

A Rust CLI tool that creates Spotify playlists from Radio Wave's daily programming. It fetches the Radio Wave playlist for a specified date and creates a corresponding Spotify playlist.

## Features

- Fetch Radio Wave playlist for a specific date
- Search for tracks on Spotify
- Automatic Spotify playlist creation
- OAuth2 authentication flow with Spotify
- Browser-based login

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
```

2. Ensure you have registered your application in the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) and added `http://127.0.0.1:8888/callback` as a redirect URI.

## Usage

Run the tool by providing a date in YYYY-MM-DD format:

```bash
spotify_cro_playlist_creator 2024-09-01
```

The tool will:
1. Open your browser for Spotify authentication
2. Fetch the Radio Wave playlist for the specified date
3. Search for matching tracks on Spotify
4. Create a new playlist titled "Radio Wave YYYY-MM-DD" - eg. "Radio Wave 2024-09-01"
5. Add all found tracks to the playlist

## API Reference

The tool uses the Radio Wave API endpoint:
```
https://api.rozhlas.cz/data/v2/playlist/day/{year}/{month}/{day}/radiowave.json
```

## License

MIT
