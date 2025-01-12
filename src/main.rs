use reqwest::Client;
use rspotify::{
    model::{misc::Market, FullTrack, Page, PlayableId, SearchResult, SearchType},
    prelude::{BaseClient, OAuthClient},
    scopes, AuthCodeSpotify, Credentials, OAuth,
};
use serde::Deserialize;
use std::env;

#[derive(Debug, PartialEq, Clone)]
struct Date {
    year: i32,
    month: u32,
    day: u32,
}

impl Date {
    fn from_str(date: &str) -> Option<Date> {
        let parts: Vec<&str> = date.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let year = parts[0].parse().ok()?;
        let month = parts[1].parse().ok()?;
        let day = parts[2].parse().ok()?;
        Some(Date { year, month, day })
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[derive(Deserialize)]
struct PlaylistItem {
    interpret: String,
    track: String,
}

#[derive(Deserialize)]
struct PlaylistResponse {
    data: Vec<PlaylistItem>,
}

#[cfg(test)]
mod test {

    #[test]
    fn test_playlist_item_deserialization() {
        let json = r#"{"since":"2024-09-01T00:03:10+02:00","id":20862650,"interpret":"LYNKS","interpret_id":33859,"track":"Tennis Song","track_id":114355,"itemcode":"9779240","files":[{"source":"gselector","id":"9779240","asset":"http:\/\/data.rozhlas.cz\/api\/v2\/asset\/cover\/gselector\/9779240.jpg","asset_width":240,"asset_height":240}]}"#;
        let item: super::PlaylistItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.interpret, "LYNKS".to_string());
        assert_eq!(item.track, "Tennis Song".to_string());
    }
}

async fn fetch_radio_playlist(date: Date) -> Result<PlaylistResponse, reqwest::Error> {
    let url = format!(
        "https://api.rozhlas.cz/data/v2/playlist/day/{:04}/{:02}/{:02}/radiowave.json",
        date.year, date.month, date.day
    );
    let client = Client::new();
    let response = client.get(&url).send().await?;
    let playlist = response.json::<PlaylistResponse>().await?;
    Ok(playlist)
}

async fn create_spotify_playlist(
    date: Date,
    tracks: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let creds = Credentials::from_env().unwrap();

    // Using every possible scope
    let scopes = scopes!(
        // "playlist-read-collaborative",
        // "playlist-read-private",
        "playlist-modify-public",
        "playlist-modify-private"
    );
    let oauth = OAuth::from_env(scopes).unwrap();

    let spotify = AuthCodeSpotify::new(creds, oauth);

    let url = spotify.get_authorize_url(false).unwrap();
    // This function requires the `cli` feature enabled.
    spotify.prompt_for_token(&url).await.unwrap();

    let user_id = spotify.me().await?.id;
    let playlist_name = format!("Radio Wave {date}");
    let playlist_description = format!("Playlist for Radio Wave for {date}");
    let playlist = spotify
        .user_playlist_create(
            user_id,
            &playlist_name,
            Some(false),
            Some(false),
            Some(&playlist_description),
        )
        .await?;

    for track in tracks {
        let search_result = spotify
            .search(
                &track,
                SearchType::Track,
                Some(Market::FromToken),
                None,
                Some(1),
                None,
            )
            .await?;

        let mayble_track = if let SearchResult::Tracks(Page { items, .. }) = search_result {
            items.first().cloned()
        } else {
            None
        };

        if let Some(FullTrack {
            id: Some(id), name, ..
        }) = mayble_track
        {
            spotify
                .playlist_add_items(playlist.id.clone(), [PlayableId::Track(id)], None)
                .await?;
            println!("- Added track: {}", name);
        } else {
            eprintln!("- Track not found: {}", track);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <date in YYYY-MM-DD format>", args[0]);
        std::process::exit(1);
    }
    let date = Date::from_str(args[1].as_str()).unwrap_or_else(|| {
        eprintln!("Invalid date format");
        std::process::exit(1);
    });

    println!("Fetching playlist for {date}");

    match fetch_radio_playlist(date.clone()).await {
        Ok(playlist) => {
            let tracks: Vec<String> = playlist
                .data
                .iter()
                .map(|item| format!("{} {}", item.interpret, item.track))
                .collect();
            if let Err(e) = create_spotify_playlist(date, tracks).await {
                eprintln!("Error creating Spotify playlist: {}", e);
            }
        }
        Err(e) => eprintln!("Error fetching radio playlist: {}", e),
    };
}
