use reqwest::Client;
use rspotify::{
    model::{
        misc::Market, FullPlaylist, FullTrack, Page, PlayableId, PlaylistId, SearchResult,
        SearchType,
    },
    prelude::{BaseClient, OAuthClient},
    AuthCodeSpotify, ClientError,
};
use serde::Deserialize;

/// A simple `YYYY-MM-DD` date.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Parse a date from a `YYYY-MM-DD` string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(date: &str) -> Option<Date> {
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
pub struct PlaylistItem {
    pub interpret: String,
    pub track: String,
}

#[derive(Deserialize)]
pub struct PlaylistResponse {
    pub data: Vec<PlaylistItem>,
}

/// Fetch the Radio Wave playlist for a given day from the Český rozhlas API.
pub async fn fetch_radio_playlist(date: &Date) -> Result<PlaylistResponse, reqwest::Error> {
    let url = format!(
        "https://api.rozhlas.cz/data/v2/playlist/day/{:04}/{:02}/{:02}/radiowave.json",
        date.year, date.month, date.day
    );
    let client = Client::new();
    let response = client.get(&url).send().await?;
    let playlist = response.json::<PlaylistResponse>().await?;
    Ok(playlist)
}

/// Build the Spotify search query for a Radio Wave playlist item.
pub fn track_query(item: &PlaylistItem) -> String {
    format!("{} {}", item.interpret, item.track)
}

/// Create an empty `Radio Wave {date}` playlist in the authenticated user's account.
pub async fn create_playlist(
    spotify: &AuthCodeSpotify,
    date: &Date,
) -> Result<FullPlaylist, ClientError> {
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
    Ok(playlist)
}

/// Result of trying to find and add a single track to a playlist.
pub enum TrackOutcome {
    Added { name: String, artists: String },
    NotFound,
}

/// Search for `query` on Spotify and, if found, add the best match to the playlist.
pub async fn find_and_add_track(
    spotify: &AuthCodeSpotify,
    playlist_id: &PlaylistId<'_>,
    query: &str,
) -> Result<TrackOutcome, ClientError> {
    // remove ft. and feat. from track name to improve search results
    let query = query.replace(" ft. ", " ").replace(" feat. ", " ");

    let search_result = spotify
        .search(
            &query,
            SearchType::Track,
            Some(Market::FromToken),
            None,
            Some(1),
            None,
        )
        .await?;

    let maybe_track = if let SearchResult::Tracks(Page { items, .. }) = search_result {
        items.first().cloned()
    } else {
        None
    };

    if let Some(FullTrack {
        id: Some(id),
        name,
        artists,
        ..
    }) = maybe_track
    {
        spotify
            .playlist_add_items(playlist_id.clone(), [PlayableId::Track(id)], None)
            .await?;
        let artists = artists
            .iter()
            .map(|a| a.name.clone())
            .collect::<Vec<String>>()
            .join(", ");
        Ok(TrackOutcome::Added { name, artists })
    } else {
        Ok(TrackOutcome::NotFound)
    }
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
