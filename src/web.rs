use std::convert::Infallible;
use std::net::SocketAddr;

use askama::Template;
use axum::{
    extract::{FromRef, Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Redirect,
    },
    routing::get,
    Router,
};
use axum_extra::extract::cookie::{Cookie, Key, PrivateCookieJar, SameSite};
use futures::StreamExt;
use rspotify::{prelude::OAuthClient, scopes, AuthCodeSpotify, Credentials, OAuth, Token};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use spotify_cro_playlist_creator::{
    create_playlist, fetch_radio_playlist, find_and_add_track, track_query, Date, TrackOutcome,
};

/// Name of the single encrypted session cookie.
const SESSION_COOKIE: &str = "session";

/// Per-visitor session state, stored (JSON-serialized) inside an encrypted cookie.
#[derive(Default, Serialize, Deserialize)]
struct SessionData {
    token: Option<Token>,
    oauth_state: Option<String>,
}

/// Shared application state.
#[derive(Clone)]
struct AppState {
    creds: Credentials,
    redirect_uri: String,
    key: Key,
}

/// Allows `PrivateCookieJar` to extract the encryption key from the app state.
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.key.clone()
    }
}

/// OAuth scopes requested from Spotify.
fn oauth_scopes() -> std::collections::HashSet<String> {
    scopes!(
        "user-read-private",
        "user-read-email",
        "playlist-modify-public",
        "playlist-modify-private"
    )
}

/// Decode the session from the encrypted cookie; missing/undecryptable -> empty.
fn read_session(jar: &PrivateCookieJar) -> SessionData {
    jar.get(SESSION_COOKIE)
        .and_then(|cookie| serde_json::from_str(cookie.value()).ok())
        .unwrap_or_default()
}

/// Serialize the session into a fresh encrypted cookie and add it to the jar.
fn write_session(jar: PrivateCookieJar, data: &SessionData) -> PrivateCookieJar {
    let value = serde_json::to_string(data).unwrap_or_default();
    let cookie = Cookie::build((SESSION_COOKIE, value))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build();
    jar.add(cookie)
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    logged_in: bool,
}

#[derive(Template)]
#[template(path = "create.html")]
struct CreateTemplate {
    date: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn index(jar: PrivateCookieJar) -> impl IntoResponse {
    let session = read_session(&jar);
    let logged_in = session.token.is_some();
    let body = IndexTemplate { logged_in }
        .render()
        .unwrap_or_else(|e| format!("Template error: {e}"));
    Html(body)
}

async fn login(State(state): State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    let mut session = read_session(&jar);
    let csrf = Uuid::new_v4().to_string();
    session.oauth_state = Some(csrf.clone());
    let jar = write_session(jar, &session);

    let oauth = OAuth {
        redirect_uri: state.redirect_uri.clone(),
        scopes: oauth_scopes(),
        state: csrf,
        ..Default::default()
    };
    let spotify = AuthCodeSpotify::new(state.creds.clone(), oauth);
    let url = spotify
        .get_authorize_url(false)
        .expect("failed to build authorize url");

    (jar, Redirect::to(&url))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(query): Query<CallbackQuery>,
) -> impl IntoResponse {
    let mut session = read_session(&jar);

    // Verify the CSRF state matches what we stored.
    if session.oauth_state.as_deref() != Some(query.state.as_str()) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid OAuth state parameter",
        )
            .into_response();
    }

    let oauth = OAuth {
        redirect_uri: state.redirect_uri.clone(),
        scopes: oauth_scopes(),
        state: query.state.clone(),
        ..Default::default()
    };
    let spotify = AuthCodeSpotify::new(state.creds.clone(), oauth);

    if let Err(e) = spotify.request_token(&query.code).await {
        return (
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Failed to obtain token: {e}"),
        )
            .into_response();
    }

    // Read the freshly obtained token out of the client.
    let token = spotify
        .token
        .lock()
        .await
        .ok()
        .and_then(|guard| guard.clone());

    session.token = token;
    session.oauth_state = None;
    let jar = write_session(jar, &session);

    (jar, Redirect::to("/")).into_response()
}

#[derive(Deserialize)]
struct DateQuery {
    date: String,
}

async fn create(jar: PrivateCookieJar, Query(query): Query<DateQuery>) -> impl IntoResponse {
    let session = read_session(&jar);
    if session.token.is_none() {
        return Redirect::to("/").into_response();
    }

    let body = CreateTemplate { date: query.date }
        .render()
        .unwrap_or_else(|e| format!("Template error: {e}"));
    Html(body).into_response()
}

async fn create_stream(jar: PrivateCookieJar, Query(query): Query<DateQuery>) -> impl IntoResponse {
    let token = read_session(&jar).token;
    let date_str = query.date;

    let inner = async_stream::stream! {
        let Some(token) = token else {
            yield Event::default()
                .event("error")
                .data("Not authenticated. Please log in again.");
            return;
        };

        let Some(date) = Date::from_str(&date_str) else {
            yield Event::default()
                .event("error")
                .data("Invalid date format. Expected YYYY-MM-DD.");
            return;
        };

        let playlist = match fetch_radio_playlist(&date).await {
            Ok(playlist) => playlist,
            Err(e) => {
                yield Event::default()
                    .event("error")
                    .data(format!("Failed to fetch Radio Wave playlist: {e}"));
                return;
            }
        };
        yield Event::default().data(format!("Found {} tracks in Radio Wave playlist", playlist.data.len()));

        let spotify = AuthCodeSpotify::from_token(token);

        let created = match create_playlist(&spotify, &date).await {
            Ok(created) => created,
            Err(e) => {
                yield Event::default()
                    .event("error")
                    .data(format!("Failed to create playlist: {e}"));
                return;
            }
        };
        yield Event::default().data(format!("Created playlist \"{}\"", created.name));

        for item in &playlist.data {
            let query = track_query(item);
            match find_and_add_track(&spotify, &created.id, &query).await {
                Ok(TrackOutcome::Added { name, artists }) => {
                    yield Event::default().data(format!("Added: {name} — {artists}"));
                }
                Ok(TrackOutcome::NotFound) => {
                    yield Event::default().data(format!("Not found: {query}"));
                }
                Err(e) => {
                    yield Event::default().data(format!("Error adding \"{query}\": {e}"));
                }
            }
        }

        let url = created
            .external_urls
            .get("spotify")
            .cloned()
            .unwrap_or_default();
        yield Event::default().event("done").data(url);
    };

    let sse_stream = inner.map(Ok::<Event, Infallible>);
    Sse::new(sse_stream).keep_alive(KeepAlive::default())
}

async fn logout(jar: PrivateCookieJar) -> impl IntoResponse {
    let jar = jar.remove(Cookie::from(SESSION_COOKIE));
    (jar, Redirect::to("/"))
}

async fn healthz() -> impl IntoResponse {
    (axum::http::StatusCode::OK, "ok")
}

// ---------------------------------------------------------------------------
// Server bootstrap
// ---------------------------------------------------------------------------

/// Build the session encryption key from `SESSION_SECRET`, or generate a random
/// one (warning: won't survive restarts / multiple instances).
fn session_key() -> Key {
    match std::env::var("SESSION_SECRET") {
        Ok(secret) if secret.len() >= 64 => Key::from(secret.as_bytes()),
        Ok(secret) if secret.len() >= 32 => {
            tracing::warn!(
                "SESSION_SECRET is shorter than 64 bytes; deriving a key from it. Use 64+ random bytes."
            );
            Key::derive_from(secret.as_bytes())
        }
        Ok(_) => {
            tracing::warn!(
                "SESSION_SECRET is too short (<32 bytes) to derive a key: generating an ephemeral key. \
                 Set SESSION_SECRET (64+ random bytes) in production."
            );
            Key::generate()
        }
        Err(_) => {
            tracing::warn!(
                "SESSION_SECRET not set: generating an ephemeral key. Sessions will not survive \
                 restarts or span multiple instances. Set SESSION_SECRET (64+ random bytes) in production."
            );
            Key::generate()
        }
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let creds = Credentials::from_env()
        .ok_or("Missing RSPOTIFY_CLIENT_ID / RSPOTIFY_CLIENT_SECRET in environment")?;
    let redirect_uri = std::env::var("RSPOTIFY_REDIRECT_URI")
        .map_err(|_| "Missing RSPOTIFY_REDIRECT_URI in environment")?;

    // Bind port: Cloud Run sets PORT. Locally, fall back to the port in the
    // redirect URI (the port Spotify redirects back to) so OAuth works without
    // extra config; otherwise default to 8080.
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .or_else(|| reqwest::Url::parse(&redirect_uri).ok().and_then(|u| u.port()))
        .unwrap_or(8080);

    let state = AppState {
        creds,
        redirect_uri,
        key: session_key(),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/login", get(login))
        .route("/callback", get(callback))
        .route("/create", get(create))
        .route("/create/stream", get(create_stream))
        .route("/logout", get(logout))
        .route("/healthz", get(healthz))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("web server listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
