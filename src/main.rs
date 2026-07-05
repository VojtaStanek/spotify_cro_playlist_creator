use rspotify::{prelude::OAuthClient, scopes, AuthCodeSpotify, Credentials, OAuth};
use std::env;
use tracing_subscriber::EnvFilter;

use spotify_cro_playlist_creator::{
    create_playlist, fetch_radio_playlist, find_and_add_track, track_query, Date, TrackOutcome,
};

mod web;

#[tokio::main]
async fn main() {
    // Load .env in local dev (no-op if the file is absent).
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = env::args().collect();

    // CLI mode: exactly one argument that parses as a YYYY-MM-DD date.
    if args.len() == 2 {
        if let Some(date) = Date::from_str(args[1].as_str()) {
            run_cli(date).await;
            return;
        }
    }

    // Otherwise: start the web server.
    if let Err(e) = web::run().await {
        eprintln!("Web server error: {e}");
        std::process::exit(1);
    }
}

/// The original interactive CLI flow, now built on the shared library helpers.
async fn run_cli(date: Date) {
    println!("Fetching playlist for {date}");

    let playlist = match fetch_radio_playlist(&date).await {
        Ok(playlist) => playlist,
        Err(e) => {
            eprintln!("Error fetching radio playlist: {e}");
            return;
        }
    };

    if let Err(e) = create_spotify_playlist(&date, &playlist).await {
        eprintln!("Error creating Spotify playlist: {e}");
    }
}

async fn create_spotify_playlist(
    date: &Date,
    playlist: &spotify_cro_playlist_creator::PlaylistResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let creds = Credentials::from_env().expect("Missing Spotify credentials in env");

    // Using every possible scope
    let scopes = scopes!(
        "user-read-private",
        "user-read-email",
        "playlist-modify-public",
        "playlist-modify-private"
    );
    let oauth = OAuth::from_env(scopes).unwrap();

    let spotify = AuthCodeSpotify::new(creds, oauth);

    let url = spotify.get_authorize_url(false).unwrap();
    // This function requires the `cli` feature enabled.
    spotify.prompt_for_token(&url).await.unwrap();

    let created = create_playlist(&spotify, date).await?;

    for item in &playlist.data {
        let query = track_query(item);
        match find_and_add_track(&spotify, &created.id, &query).await? {
            TrackOutcome::Added { name, artists } => {
                println!("- Added track: {name} by {artists} (CRo playlist entry: {query})");
            }
            TrackOutcome::NotFound => {
                eprintln!("- Track not found: {query}");
            }
        }
    }

    Ok(())
}
