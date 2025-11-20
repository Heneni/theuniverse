use std::{cmp::Reverse, convert::Infallible, sync::Arc, time::Instant};

use chrono::{NaiveDateTime, Utc};
use diesel::{self, prelude::*};
use fnv::{FnvHashMap as HashMap, FnvHashSet};
use futures::{stream::FuturesUnordered, StreamExt, TryFutureExt, TryStreamExt};
use redis::Commands;
use rocket::{
    data::ToByteUnit,
    http::{RawStr, Status},
    request::Outcome,
    response::{status, Redirect},
    serde::json::Json,
    State,
};
use tokio::{
    sync::Mutex,
    task::{block_in_place, spawn_blocking},
};

use crate::{
    artist_embedding::{
        get_artist_embedding_ctx, get_average_artists,
        map_3d::{get_map_3d_artist_ctx, get_packed_3d_artist_coords},
        ArtistEmbeddingError,
    },
    benchmarking::{mark, start},
    cache::{get_hash_items, get_redis_conn, set_hash_items},
    conf::CONF,
    db_util::{
        self, get_all_top_artists_for_user, get_artist_spotify_ids_by_internal_id,
        get_internal_ids_by_spotify_id, insert_related_artists,
    },
    metrics::{endpoint_response_time, user_updates_failure_total, user_updates_success_total},
    models::{
        Artist, ArtistSearchResult, AverageArtistItem, AverageArtistsResponse, CompareToRequest,
        CreateSharedPlaylistRequest, NewRelatedArtistEntry, NewUser, OAuthTokenResponse, Playlist,
        RelatedArtistsGraph, StatsSnapshot, TimeFrames, Timeline, TimelineEvent, TimelineEventType,
        Track, User, UserComparison,
    },
    spotify_api::{
        fetch_artists, fetch_top_tracks_for_artist, get_multiple_related_artists,
        get_reqwest_client, search_artists,
    },
    DbConn, SpotifyTokenData,
};

const SPOTIFY_TOKEN_FETCH_URL: &str = "https://accounts.spotify.com/api/token";

#[get("/")]
pub(crate) fn index() -> &'static str { "Application successfully started!" }

/// Retrieves the current top tracks and artist for the current user (now uses CSV data)
#[get("/stats/<username>")]
#[allow(unused_variables)]
pub(crate) async fn get_current_stats(
    conn: DbConn,
    conn2: DbConn,
    username: String,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<Option<Json<StatsSnapshot>>, String> {
    let start_tok = start();

    // Load data from CSV instead of database
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    let mut snapshot = StatsSnapshot::new(chrono::Utc::now().naive_utc());

    // Add top artists
    for (timeframe_id, artist_ids) in [
        (0, &csv_data.top_artists_short),
        (1, &csv_data.top_artists_medium),
        (2, &csv_data.top_artists_long),
    ] {
        for artist_name in artist_ids {
            let artist_id = format!("csv_{}", artist_name.replace(' ', "_").to_lowercase());
            if let Some(artist) = csv_data.artists.get(&artist_id) {
                snapshot.artists.add_item_by_id(timeframe_id, artist.clone());
            }
        }
    }

    // Add top tracks
    for (timeframe_id, track_ids) in [
        (0, &csv_data.top_tracks_short),
        (1, &csv_data.top_tracks_medium),
        (2, &csv_data.top_tracks_long),
    ] {
        for track_key in track_ids {
            let track_id = format!("csv_{}", track_key.replace(' ', "_").to_lowercase());
            if let Some(track) = csv_data.tracks.get(&track_id) {
                snapshot.tracks.add_item_by_id(timeframe_id, track.clone());
            }
        }
    }

    endpoint_response_time("get_current_stats").observe(start_tok.elapsed().as_nanos() as u64);

    Ok(Some(Json(snapshot)))
}

#[derive(Serialize)]
pub(crate) struct ArtistStats {
    pub artist: Artist,
    pub tracks_by_id: HashMap<String, Track>,
    pub popularity_history: Vec<(NaiveDateTime, [Option<u8>; 3])>,
    pub top_tracks: Vec<(String, usize)>,
}

#[get("/stats/<username>/artist/<artist_id>")]
#[allow(unused_variables)]
pub(crate) async fn get_artist_stats(
    conn: DbConn,
    conn2: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    username: String,
    artist_id: String,
) -> Result<Option<Json<ArtistStats>>, String> {
    let start_tok = start();
    
    // Load data from CSV instead of database
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    // Get the artist
    let artist = match csv_data.artists.get(&artist_id) {
        Some(artist) => artist.clone(),
        None => return Ok(None),
    };

    // Find tracks by this artist
    let mut tracks_by_id: HashMap<String, Track> = HashMap::default();
    let mut top_track_scores: Vec<(String, usize)> = Vec::new();
    
    for (track_id, track) in &csv_data.tracks {
        if track.artists.iter().any(|a| a.id == artist_id) {
            tracks_by_id.insert(track_id.clone(), track.clone());
            
            // Calculate play count for this track from CSV
            let track_name = &track.name;
            let play_count = csv_data.entries.iter()
                .filter(|e| &e.track_name == track_name && e.artist_name == artist.name)
                .count();
            
            if play_count > 0 {
                top_track_scores.push((track_id.clone(), play_count));
            }
        }
    }
    
    // Sort by play count
    top_track_scores.sort_by(|a, b| b.1.cmp(&a.1));
    top_track_scores.truncate(20);

    // For popularity history, create a simple static history
    // In a real implementation, this would be computed from CSV timestamp data
    let popularity_history: Vec<(NaiveDateTime, [Option<u8>; 3])> = Vec::new();

    let stats = ArtistStats {
        artist,
        tracks_by_id,
        popularity_history,
        top_tracks: top_track_scores,
    };
    
    endpoint_response_time("get_artists_stats").observe(start_tok.elapsed().as_nanos() as u64);
    Ok(Some(Json(stats)))
}

#[derive(Serialize)]
pub(crate) struct GenresHistory {
    pub timestamps: Vec<NaiveDateTime>,
    pub history_by_genre: HashMap<String, Vec<Option<usize>>>,
}

#[get("/stats/<username>/genre_history")]
#[allow(unused_variables)]
pub(crate) async fn get_genre_history(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    username: String,
) -> Result<Option<Json<GenresHistory>>, String> {
    let start = Instant::now();
    
    // Load data from CSV instead of database
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    // Convert CSV genre history to the expected format
    let mut timestamps = Vec::new();
    let mut history_by_genre: HashMap<String, Vec<Option<usize>>> = HashMap::default();

    for (timestamp, genre_counts) in &csv_data.genre_history {
        timestamps.push(timestamp.naive_utc());
        
        // For each genre, calculate its ranking for this timestamp
        let mut genre_vec: Vec<(String, f32)> = genre_counts.iter()
            .map(|(genre, count)| (genre.clone(), *count))
            .collect();
        genre_vec.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Update history for each genre
        for genre in genre_counts.keys() {
            let ranking = genre_vec.iter().position(|(g, _)| g == genre);
            history_by_genre.entry(genre.clone())
                .or_insert_with(|| vec![None; timestamps.len() - 1])
                .push(ranking);
        }
        
        // Fill in None for genres not present in this timestamp
        for (genre, history) in history_by_genre.iter_mut() {
            if history.len() < timestamps.len() {
                history.push(None);
            }
        }
    }

    endpoint_response_time("get_genre_history").observe(start.elapsed().as_nanos() as u64);
    Ok(Some(Json(GenresHistory {
        timestamps,
        history_by_genre,
    })))
}

#[derive(Serialize)]
pub(crate) struct GenreStats {
    pub artists_by_id: HashMap<String, Artist>,
    pub top_artists: Vec<(String, f32)>,
    pub timestamps: Vec<NaiveDateTime>,
    pub popularity_history: TimeFrames<usize>,
}

#[get("/stats/<username>/genre/<genre>")]
#[allow(unused_variables)]
pub(crate) async fn get_genre_stats(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    username: String,
    genre: String,
) -> Result<Option<Json<GenreStats>>, String> {
    let start = Instant::now();
    
    // Load data from CSV instead of database
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    // Find artists with this genre
    let mut artists_by_id: HashMap<String, Artist> = HashMap::default();
    let mut artist_play_counts: HashMap<String, f32> = HashMap::default();
    
    let genre_lower = genre.to_lowercase();
    for (artist_id, artist) in &csv_data.artists {
        if let Some(genres) = &artist.genres {
            if genres.iter().any(|g| g.to_lowercase() == genre_lower) {
                artists_by_id.insert(artist_id.clone(), artist.clone());
                
                // Count plays for this artist
                let play_count: u64 = csv_data.entries.iter()
                    .filter(|e| e.artist_name == artist.name)
                    .map(|e| e.ms_played)
                    .sum();
                artist_play_counts.insert(artist_id.clone(), play_count as f32);
            }
        }
    }

    // Sort artists by play count
    let mut top_artists: Vec<(String, f32)> = artist_play_counts.into_iter().collect();
    top_artists.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    top_artists.truncate(20);

    // Create empty popularity history (would need historical data to populate this properly)
    let popularity_history = TimeFrames {
        short: vec![],
        medium: vec![],
        long: vec![],
    };
    
    // Extract timestamps from genre history
    let timestamps: Vec<NaiveDateTime> = csv_data.genre_history.iter()
        .map(|(dt, _)| dt.naive_utc())
        .collect();

    endpoint_response_time("get_genre_stats").observe(start.elapsed().as_nanos() as u64);

    Ok(Some(Json(GenreStats {
        artists_by_id,
        top_artists,
        popularity_history,
        timestamps,
    })))
}

#[get("/stats/<username>/timeline?<start_day_id>&<end_day_id>")]
#[allow(unused_variables)]
pub(crate) async fn get_timeline(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    conn_2: DbConn,
    username: String,
    start_day_id: String,
    end_day_id: String,
) -> Result<Option<Json<Timeline>>, String> {
    let start = Instant::now();
    let start_day = NaiveDateTime::parse_from_str(
        &format!("{}T08:00:00+08:00", start_day_id),
        "%Y-%m-%dT%H:%M:%S%z",
    )
    .map_err(|_| String::from("Invalid `start_day_id` provided"))?;
    let end_day = NaiveDateTime::parse_from_str(
        &format!("{}T08:00:00+08:00", end_day_id),
        "%Y-%m-%dT%H:%M:%S%z",
    )
    .map_err(|_| String::from("Invalid `end_day_id` provided"))?;

    // Load data from CSV instead of database
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    let mut events = Vec::new();
    let mut event_count = 0;

    // Filter artist first seen events by date range
    for (artist_id, first_seen_dt) in &csv_data.artist_first_seen {
        let first_seen = first_seen_dt.naive_utc();
        if first_seen >= start_day && first_seen <= end_day {
            if let Some(artist) = csv_data.artists.get(artist_id) {
                event_count += 1;
                events.push(TimelineEvent {
                    event_type: TimelineEventType::ArtistFirstSeen {
                        artist: artist.clone(),
                    },
                    date: first_seen.date(),
                    id: event_count,
                });
            }
        }
    }

    // Filter track first seen events by date range
    for (track_id, first_seen_dt) in &csv_data.track_first_seen {
        let first_seen = first_seen_dt.naive_utc();
        if first_seen >= start_day && first_seen <= end_day {
            if let Some(track) = csv_data.tracks.get(track_id) {
                event_count += 1;
                events.push(TimelineEvent {
                    event_type: TimelineEventType::TopTrackFirstSeen {
                        track: track.clone(),
                    },
                    date: first_seen.date(),
                    id: event_count,
                });
            }
        }
    }

    events.sort_unstable_by_key(|evt| evt.date);
    endpoint_response_time("get_timeline").observe(start.elapsed().as_nanos() as u64);

    Ok(Some(Json(Timeline { events })))
}

/// Redirects to the Spotify authorization page for the application
/// OAuth authorization route - now stubbed out (returns to home page)
#[get("/authorize?<playlist_perms>&<state>")]
#[allow(unused_variables)]
pub(crate) fn authorize(playlist_perms: Option<&str>, state: Option<&str>) -> Redirect {
    // Instead of redirecting to Spotify, redirect back to the home page
    // since we're now using local CSV data
    Redirect::to(format!("{}", CONF.website_url))
}

/// The playlist will be generated on the account of user2
async fn generate_shared_playlist(
    conn1: DbConn,
    conn2: DbConn,
    conn3: DbConn,
    conn4: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    bearer_token: &str,
    user1: &str,
    user2: &str,
) -> Result<Option<Playlist>, String> {
    let start = Instant::now();
    let (user1_res, user2_res) = tokio::join!(
        async move {
            db_util::get_user_by_spotify_id(&conn1, user1.to_owned())
                .await
                .map(|user_opt| user_opt.map(|user| (user, conn1)))
        },
        async move {
            db_util::get_user_by_spotify_id(&conn2, user2.to_owned())
                .await
                .map(|user_opt| user_opt.map(|user| (user, conn2)))
        },
    );
    let (user1, conn1) = match user1_res? {
        Some(user) => user,
        None => {
            return Ok(None);
        },
    };
    let (mut user2, conn2) = match user2_res? {
        Some(user) => user,
        None => {
            return Ok(None);
        },
    };

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    if let Some(res) = db_util::refresh_user_access_token(&conn1, &mut user2).await? {
        error!("Error refreshing access token: {:?}", res);
        return Err("Error refreshing access token".to_string());
    }

    let playlist_track_spotify_ids =
        crate::shared_playlist_gen::generate_shared_playlist_track_spotify_ids(
            conn1,
            conn2,
            conn3,
            conn4,
            &user1,
            &user2,
            &spotify_access_token,
        )
        .await?;

    let created_playlist = crate::spotify_api::create_playlist(
        bearer_token,
        &user2,
        format!("Shared Tastes of {} and {}", user1.username, user2.username),
        Some(format!(
            "Contains tracks and artists that both {} and {} enjoy, {}",
            user1.username, user2.username, "generated by spotifytrack.net"
        )),
        &playlist_track_spotify_ids,
    )
    .await?;

    endpoint_response_time("generate_shared_playlist").observe(start.elapsed().as_nanos() as u64);
    Ok(Some(created_playlist))
}

/// This handles the OAuth authentication process for new users.  It is hit as the callback for the
/// OAuth callback - now stubbed out (redirects to demo user stats)
#[get("/oauth_cb?<error>&<code>&<state>")]
#[allow(unused_variables)]
pub(crate) async fn oauth_cb(
    conn1: DbConn,
    conn2: DbConn,
    conn3: DbConn,
    conn4: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    error: Option<&str>,
    code: &str,
    state: Option<&str>,
) -> Result<Redirect, String> {
    // Since we're using local CSV data, redirect to a demo user stats page
    let redirect_url = format!("{}/stats/demo", CONF.website_url);
    Ok(Redirect::to(redirect_url))
}

/// Returns `true` if the token is valid, false if it's not
async fn validate_api_token(api_token_data: rocket::data::Data<'_>) -> Result<bool, String> {
    let api_token = api_token_data
        .open(1usize.mebibytes())
        .into_string()
        .await
        .map_err(|err| {
            error!("Error reading provided admin API token: {:?}", err);
            String::from("Error reading post data body")
        })?
        .into_inner();
    Ok(api_token == CONF.admin_api_token)
}

async fn update_user_inner(
    conn: &DbConn,
    user_id: Option<String>,
) -> Result<(), status::Custom<String>> {
    use crate::schema::users::dsl::*;

    // Get the least recently updated user
    let mut user: User = match user_id.clone().map(|s| -> Result<String, _> {
        let s = RawStr::new(s.as_str());
        match s.percent_decode() {
            Ok(decoded) => Ok(decoded.into()),
            Err(err) => Err(err),
        }
    }) {
        Some(s) => {
            let user_id: String = s.map_err(|_| {
                error!("Invalid `user_id` param provided to `/update/user`");
                status::Custom(
                    Status::BadRequest,
                    String::from("Invalid `user_id` param; couldn't decode"),
                )
            })?;

            conn.run(move |conn| users.filter(spotify_id.eq(user_id)).first(conn))
                .await
        },
        None =>
            conn.run(move |conn| users.order_by(last_update_time).first(conn))
                .await,
    }
    .map_err(|err| {
        error!("{:?}", err);
        status::Custom(
            Status::InternalServerError,
            "Error querying user to update from database".into(),
        )
    })?;

    if let Some(res) = db_util::refresh_user_access_token(&conn, &mut user)
        .await
        .map_err(|err| status::Custom(Status::InternalServerError, err))?
    {
        return Err(res);
    }

    // Only update the user if it's been longer than the minimum update interval
    let min_update_interval_seconds = crate::conf::CONF.min_update_interval;
    let now = chrono::Utc::now().naive_utc();
    let diff = now - user.last_update_time;
    if user_id.is_none() && diff < min_update_interval_seconds {
        let msg = format!(
            "{} since last update; not updating anything right now.",
            diff
        );
        info!("{}", msg);
        return Err(status::Custom(Status::Ok, msg));
    }
    info!("{diff} since last update; proceeding with update.");

    if let Err(err) =
        crate::db_util::update_user_last_updated(&user, &conn, Utc::now().naive_utc()).await
    {
        error!(
            "Error updating user {:?} last updated time: {:?}",
            user, err
        );
        return Err(status::Custom(
            Status::InternalServerError,
            "Error updating user last updated time".into(),
        ));
    }

    let stats = match crate::spotify_api::fetch_cur_stats(&user).await {
        Ok(Some(stats)) => stats,
        Ok(None) => {
            error!(
                "Error when fetching stats for user {:?}; no stats returned.",
                user
            );
            return Err(status::Custom(
                Status::InternalServerError,
                "No data from Spotify API for that user".into(),
            ));
        },
        Err(err) => {
            error!("Error fetching user stats: {:?}", err);
            return Err(status::Custom(
                Status::InternalServerError,
                "Error fetching user stats".into(),
            ));
        },
    };

    crate::spotify_api::store_stats_snapshot(&conn, &user, stats)
        .await
        .map_err(|err| status::Custom(Status::InternalServerError, err))?;

    info!("Successfully updated user {}", user.spotify_id);

    Ok(())
}

/// This route is internal and hit by the cron job that is called to periodically update the stats
/// for the least recently updated user.
#[post("/update_user?<user_id>&<count>", data = "<api_token_data>")]
pub(crate) async fn update_user(
    conn: DbConn,
    api_token_data: rocket::data::Data<'_>,
    user_id: Option<String>,
    count: Option<usize>,
) -> Result<status::Custom<String>, String> {
    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    if let Some(user_id) = user_id {
        if let Err(status) = update_user_inner(&conn, Some(user_id)).await {
            user_updates_failure_total().inc();
            return Ok(status);
        }
        user_updates_success_total().inc();
        return Ok(status::Custom(Status::Ok, "User updated".into()));
    }

    let count = count.unwrap_or(1);
    let mut success_count = 0usize;
    let mut fail_count = 0usize;
    for _ in 0..count {
        let success = update_user_inner(&conn, None).await.is_ok();
        if success {
            user_updates_success_total().inc();
            success_count += 1;
        } else {
            user_updates_failure_total().inc();
            fail_count += 1;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    Ok(status::Custom(
        Status::Ok,
        format!(
            "Successfully updated {} user(s); failed to update {} user(s)",
            success_count, fail_count
        ),
    ))
}

#[post("/populate_tracks_artists_mapping_table", data = "<api_token_data>")]
pub(crate) async fn populate_tracks_artists_mapping_table(
    conn: DbConn,
    api_token_data: rocket::data::Data<'_>,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<status::Custom<String>, String> {
    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    crate::db_util::populate_tracks_artists_table(&conn, &spotify_access_token).await?;

    Ok(status::Custom(
        Status::Ok,
        "Sucessfully populated mapping table".into(),
    ))
}

#[post("/populate_artists_genres_mapping_table", data = "<api_token_data>")]
pub(crate) async fn populate_artists_genres_mapping_table(
    conn: DbConn,
    api_token_data: rocket::data::Data<'_>,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<status::Custom<String>, String> {
    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    crate::db_util::populate_artists_genres_table(&conn, &spotify_access_token).await?;

    Ok(status::Custom(
        Status::Ok,
        "Sucessfully populated mapping table".into(),
    ))
}

async fn compute_comparison(
    user1: String,
    user2: String,
    conn1: DbConn,
    conn2: DbConn,
    conn3: DbConn,
    conn4: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<Option<UserComparison>, String> {
    let (user1_res, user2_res) = tokio::join!(
        async move {
            db_util::get_user_by_spotify_id(&conn1, user1)
                .await
                .map(|user_opt| user_opt.map(|user| (user, conn1)))
        },
        async move {
            db_util::get_user_by_spotify_id(&conn2, user2)
                .await
                .map(|user_opt| user_opt.map(|user| (user, conn2)))
        },
    );
    let (user1, conn1) = match user1_res? {
        Some(user) => user,
        None => {
            return Ok(None);
        },
    };
    let (user2, conn2) = match user2_res? {
        Some(user) => user,
        None => {
            return Ok(None);
        },
    };
    let (user1_id, user2_id) = (user1.id, user2.id);

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;
    let spotify_access_token_clone = spotify_access_token.clone();

    let stats = tokio::try_join!(
        crate::db_util::get_all_top_tracks_for_user(&conn1, user1_id)
            .map_err(db_util::stringify_diesel_err),
        crate::db_util::get_all_top_tracks_for_user(&conn2, user2_id)
            .map_err(db_util::stringify_diesel_err),
        crate::db_util::get_all_top_artists_for_user(&conn3, user1_id)
            .map_err(db_util::stringify_diesel_err),
        crate::db_util::get_all_top_artists_for_user(&conn4, user2_id)
            .map_err(db_util::stringify_diesel_err),
    )?;
    let (user1_tracks, user2_tracks, user1_artists, user2_artists) = stats;

    let tracks_intersection = async move {
        let mut intersection = user1_tracks;
        intersection.retain(|(id, _)| user2_tracks.iter().any(|(o_id, _)| *o_id == *id));

        let spotify_ids = intersection
            .iter()
            .map(|(_, spotify_id)| spotify_id.as_str())
            .collect::<Vec<_>>();
        crate::spotify_api::fetch_tracks(&spotify_access_token, &spotify_ids).await
    };
    let artists_intersection = async move {
        let mut intersection = user1_artists;
        intersection.retain(|(id, _)| user2_artists.iter().any(|(o_id, _)| *o_id == *id));

        let spotify_ids = intersection
            .iter()
            .map(|(_, spotify_id)| spotify_id.as_str())
            .collect::<Vec<_>>();
        crate::spotify_api::fetch_artists(&spotify_access_token_clone, &spotify_ids).await
    };
    let intersections = tokio::try_join!(tracks_intersection, artists_intersection)?;
    let (tracks_intersection, artists_intersection) = intersections;

    Ok(Some(UserComparison {
        tracks: tracks_intersection,
        artists: artists_intersection,
        genres: Vec::new(), // TODO
        user1_username: user1.username,
        user2_username: user2.username,
    }))
}

#[get("/compare/<user1>/<user2>")]
#[allow(unused_variables)]
pub(crate) async fn compare_users(
    conn1: DbConn,
    conn2: DbConn,
    conn3: DbConn,
    conn4: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    user1: String,
    user2: String,
) -> Result<Option<Json<UserComparison>>, String> {
    let start = Instant::now();
    
    // In CSV-only mode, comparing two users doesn't make sense since we only have one CSV
    // Return empty comparison
    endpoint_response_time("compare_users").observe(start.elapsed().as_nanos() as u64);
    Ok(Some(Json(UserComparison {
        tracks: Vec::new(),
        artists: Vec::new(),
        genres: Vec::new(),
        user1_username: user1,
        user2_username: user2,
    })))
}

async fn build_related_artists_graph(
    spotify_access_token: String,
    artist_ids: &[&str],
) -> Result<RelatedArtistsGraph, String> {
    // Get related artists for all of them
    let related_artists =
        get_multiple_related_artists(spotify_access_token.clone(), artist_ids).await?;

    let all_artist_ids: FnvHashSet<String> = artist_ids
        .iter()
        .copied()
        .map(String::from)
        .chain(
            related_artists
                .iter()
                .flat_map(|related_artists| related_artists.iter().cloned()),
        )
        .collect();

    let mut related_artists_by_id = HashMap::default();
    for (&artist_id, related_artists) in artist_ids.into_iter().zip(related_artists.iter()) {
        related_artists_by_id.insert(artist_id.to_owned(), related_artists.clone());
    }

    let all_artist_ids: Vec<_> = all_artist_ids.iter().map(String::as_str).collect();
    let extra_artists_list = fetch_artists(&spotify_access_token, &all_artist_ids).await?;
    let mut extra_artists = HashMap::default();
    for artist in extra_artists_list {
        extra_artists.insert(artist.id.clone(), artist);
    }

    Ok(RelatedArtistsGraph {
        extra_artists,
        related_artists: related_artists_by_id,
    })
}

#[get("/stats/<user_id>/related_artists_graph")]
#[allow(unused_variables)]
pub(crate) async fn get_related_artists_graph(
    conn: DbConn,
    user_id: String,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<Option<Json<RelatedArtistsGraph>>, String> {
    let start = Instant::now();
    
    // Load data from CSV instead of database
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    // Get top artists from all timeframes
    let mut all_artist_ids: FnvHashSet<String> = FnvHashSet::default();
    for artist_name in csv_data.top_artists_short.iter()
        .chain(csv_data.top_artists_medium.iter())
        .chain(csv_data.top_artists_long.iter())
    {
        let artist_id = format!("csv_{}", artist_name.replace(' ', "_").to_lowercase());
        all_artist_ids.insert(artist_id);
    }

    // Build related artists graph from CSV data
    let mut related_artists_by_id = HashMap::default();
    let mut extra_artists = HashMap::default();

    for artist_id in &all_artist_ids {
        if let Some(artist) = csv_data.artists.get(artist_id) {
            extra_artists.insert(artist_id.clone(), artist.clone());
            
            // Get related artists from our pre-computed relationships
            if let Some(related_ids) = csv_data.artist_relationships.get(artist_id) {
                related_artists_by_id.insert(artist_id.clone(), related_ids.clone());
                
                // Add related artists to extra_artists
                for related_id in related_ids {
                    if let Some(related_artist) = csv_data.artists.get(related_id) {
                        extra_artists.insert(related_id.clone(), related_artist.clone());
                    }
                }
            }
        }
    }

    let out = RelatedArtistsGraph {
        extra_artists,
        related_artists: related_artists_by_id,
    };
    
    endpoint_response_time("get_related_artists_graph").observe(start.elapsed().as_nanos() as u64);
    Ok(Some(Json(out)))
}

#[get("/related_artists/<artist_id>")]
#[allow(unused_variables)]
pub(crate) async fn get_related_artists(
    artist_id: String,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<Option<Json<RelatedArtistsGraph>>, String> {
    let start = Instant::now();
    
    // Load data from CSV instead of Spotify API
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    let mut extra_artists = HashMap::default();
    let mut related_artists_by_id = HashMap::default();

    // Get the artist
    if let Some(artist) = csv_data.artists.get(&artist_id) {
        extra_artists.insert(artist_id.clone(), artist.clone());
        
        // Get related artists from pre-computed relationships
        if let Some(related_ids) = csv_data.artist_relationships.get(&artist_id) {
            related_artists_by_id.insert(artist_id.clone(), related_ids.clone());
            
            // Add related artists to extra_artists
            for related_id in related_ids {
                if let Some(related_artist) = csv_data.artists.get(related_id) {
                    extra_artists.insert(related_id.clone(), related_artist.clone());
                }
            }
        }
    }

    let out = RelatedArtistsGraph {
        extra_artists,
        related_artists: related_artists_by_id,
    };
    
    endpoint_response_time("get_related_artists").observe(start.elapsed().as_nanos() as u64);
    Ok(Some(Json(out)))
}

#[get("/display_name/<username>")]
#[allow(unused_variables)]
pub(crate) async fn get_display_name(
    conn: DbConn,
    username: String,
) -> Result<Option<String>, String> {
    let start = Instant::now();
    
    // In CSV-only mode, just return the username as display name
    endpoint_response_time("get_display_name").observe(start.elapsed().as_nanos() as u64);
    Ok(Some(username))
}

#[post("/dump_redis_related_artists_to_database", data = "<api_token_data>")]
pub(crate) async fn dump_redis_related_artists_to_database(
    conn: DbConn,
    api_token_data: rocket::Data<'_>,
) -> Result<status::Custom<String>, String> {
    let start = Instant::now();

    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    let mut redis_conn = get_redis_conn()?;
    let all_values: Vec<String> = block_in_place(|| redis_conn.hgetall("related_artists"))
        .map_err(|err| {
            error!("Error with HGETALL on related artists data: {:?}", err);
            String::from("Redis error")
        })?;

    let mut all_mapped_spotify_ids: HashMap<String, i32> = HashMap::default();

    for chunk in all_values.chunks(200) {
        let mapped_spotify_ids =
            get_internal_ids_by_spotify_id(&conn, chunk.chunks_exact(2).map(|chunk| &chunk[0]))
                .await
                .map_err(|err| {
                    error!("Error mapping spotify ids: {:?}", err);
                    String::from("Error mapping spotify ids")
                })?;

        for (k, v) in mapped_spotify_ids {
            all_mapped_spotify_ids.insert(k, v);
        }
    }

    let entries: Vec<NewRelatedArtistEntry> = all_values
        .chunks_exact(2)
        .map(|val| {
            let artist_spotify_id = &val[0];
            let related_artists_json = val[1].clone();
            let artist_spotify_id = *all_mapped_spotify_ids
                .get(artist_spotify_id)
                .expect("Spotify ID didn't get mapped");

            NewRelatedArtistEntry {
                artist_spotify_id,
                related_artists_json,
            }
        })
        .collect();

    for chunk in entries.chunks(200) {
        insert_related_artists(&conn, chunk.into())
            .await
            .map_err(|err| {
                error!("DB error inserting related artist into DB: {:?}", err);
                String::from("DB error")
            })?;
    }

    endpoint_response_time("dump_redis_related_artists_to_database")
        .observe(start.elapsed().as_nanos() as u64);

    Ok(status::Custom(
        Status::Ok,
        String::from("Successfully dumped all related artists from Redis to MySQL"),
    ))
}

#[post("/crawl_related_artists", data = "<api_token_data>")]
pub(crate) async fn crawl_related_artists(
    api_token_data: rocket::Data<'_>,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<status::Custom<String>, String> {
    let start = Instant::now();

    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let mut redis_conn = get_redis_conn()?;
    let artist_ids: Vec<String> = block_in_place(|| {
        redis::cmd("HRANDFIELD")
            .arg("related_artists")
            .arg("8")
            .query::<Vec<String>>(&mut *redis_conn)
    })
    .map_err(|err| {
        error!(
            "Error getting random related artist keys from Redis cache: {:?}",
            err
        );
        String::from("Redis error")
    })?;

    let mut all_related_artists: Vec<String> = Vec::new();

    let related_artists_jsons: Vec<String> = block_in_place(|| {
        redis_conn
            .hget("related_artists", artist_ids)
            .map_err(|err| {
                error!("Error getting related artist from Redis: {:?}", err);
                String::from("Redis error")
            })
    })?;

    for related_artists_json in related_artists_jsons {
        let Ok(related_artist_ids) = serde_json::from_str::<Vec<String>>(&related_artists_json)
        else {
            error!(
                "Invalid entry in related artists Redis; can't parse into array of strings; \
                 found={}",
                related_artists_json
            );
            continue;
        };

        all_related_artists.extend(related_artist_ids.into_iter());
    }

    info!("Crawling {} related artists...", all_related_artists.len());
    let mut all_related_artists: Vec<&str> =
        all_related_artists.iter().map(String::as_str).collect();
    all_related_artists.sort_unstable();
    all_related_artists.dedup();

    let fetched =
        get_multiple_related_artists(spotify_access_token.clone(), &all_related_artists).await?;
    endpoint_response_time("crawl_related_artists").observe(start.elapsed().as_nanos() as u64);
    Ok(status::Custom(
        Status::Ok,
        format!(
            "Successfully fetched {} related artists to poulate related artists Redis hash",
            fetched.len()
        ),
    ))
}

pub(crate) struct UserAgent(String);

#[async_trait]
impl<'a, 'r> rocket::request::FromRequest<'r> for UserAgent {
    type Error = Infallible;

    async fn from_request(
        req: &'r rocket::request::Request<'_>,
    ) -> rocket::request::Outcome<Self, Self::Error> {
        let token = req.headers().get_one("user-agent");
        match token {
            Some(token) => Outcome::Success(UserAgent(token.to_string())),
            None => Outcome::Success(UserAgent(String::new())),
        }
    }
}

#[get("/search_artist?<q>")]
#[allow(unused_variables)]
pub(crate) async fn search_artist(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    q: String,
    user_agent: UserAgent,
) -> Result<Json<Vec<ArtistSearchResult>>, String> {
    let start = Instant::now();
    
    // Load data from CSV instead of Spotify API
    let csv_data = crate::csv_loader::get_csv_data()
        .await
        .ok_or_else(|| "CSV data not loaded".to_string())?;

    // Simple case-insensitive search through CSV artists
    let query_lower = q.to_lowercase();
    let mut results: Vec<ArtistSearchResult> = Vec::new();

    for (artist_id, artist) in &csv_data.artists {
        if artist.name.to_lowercase().contains(&query_lower) {
            results.push(ArtistSearchResult {
                spotify_id: artist_id.clone(),
                internal_id: None,
                name: artist.name.clone(),
            });
            
            // Limit to 20 results
            if results.len() >= 20 {
                break;
            }
        }
    }

    endpoint_response_time("search_artist").observe(start.elapsed().as_nanos() as u64);
    Ok(Json(results))
}

#[get(
    "/average_artists/<artist_1_spotify_id>/<artist_2_spotify_id>?<count>&<artist_1_bias>&\
     <artist_2_bias>"
)]
pub(crate) async fn get_average_artists_route(
    conn: DbConn,
    artist_1_spotify_id: String,
    artist_2_spotify_id: String,
    count: Option<usize>,
    artist_1_bias: Option<f32>,
    artist_2_bias: Option<f32>,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<Json<AverageArtistsResponse>, String> {
    let start = Instant::now();

    // Look up internal IDs for provided spotify IDs
    let internal_ids_by_spotify_id = get_internal_ids_by_spotify_id(
        &conn,
        [artist_1_spotify_id.clone(), artist_2_spotify_id.clone()].iter(),
    )
    .await?;
    let artist_1_id = match internal_ids_by_spotify_id.get(&artist_1_spotify_id) {
        Some(id) => *id,
        None => return Err(format!("No artist found with id={}", artist_1_spotify_id)),
    };
    let artist_2_id = match internal_ids_by_spotify_id.get(&artist_2_spotify_id) {
        Some(id) => *id,
        None => return Err(format!("No artist found with id={}", artist_2_spotify_id)),
    };
    let count = count.unwrap_or(10).min(50);
    assert!(artist_1_id > 0);
    assert!(artist_2_id > 0);

    let mut average_artists = match get_average_artists(
        artist_1_id as usize,
        artist_1_bias.unwrap_or(1.),
        artist_2_id as usize,
        artist_2_bias.unwrap_or(1.),
        count,
    ) {
        Ok(res) => res,
        Err(err) => match err {
            ArtistEmbeddingError::ArtistIdNotFound(id) =>
                return Err(format!(
                    "No artist found in embedding with internal id={}",
                    id
                )),
        },
    };

    let all_artist_internal_ids: Vec<i32> = average_artists.iter().map(|d| d.id as i32).collect();
    let artist_spotify_ids_by_internal_id: HashMap<i32, String> =
        get_artist_spotify_ids_by_internal_id(&conn, all_artist_internal_ids)
            .await
            .map_err(|err| {
                error!(
                    "Error converting artist internal ids to spotify ids after performing \
                     averaging: {:?}",
                    err
                );
                String::from("Internal database error")
            })?;

    let all_spotify_ids: Vec<&str> = artist_spotify_ids_by_internal_id
        .values()
        .map(String::as_str)
        .collect();

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let top_tracks_for_artists = FuturesUnordered::new();
    for artist_spotify_id in &all_spotify_ids {
        let artist_spotify_id_clone = String::from(*artist_spotify_id);
        top_tracks_for_artists.push(
            fetch_top_tracks_for_artist(&spotify_access_token, artist_spotify_id)
                .map_ok(move |res| (artist_spotify_id_clone, res)),
        );
    }

    let (top_tracks, fetched_artists) = tokio::try_join!(
        top_tracks_for_artists.try_collect::<Vec<_>>(),
        fetch_artists(&spotify_access_token, &all_spotify_ids)
    )?;
    let mut top_tracks_by_artist_spotify_id: HashMap<String, Vec<Track>> =
        top_tracks.into_iter().collect();

    if fetched_artists.len() != average_artists.len() {
        assert!(fetched_artists.len() < average_artists.len());
        average_artists.retain(|d| {
            let avg_artist_spotify_id = match artist_spotify_ids_by_internal_id.get(&(d.id as i32))
            {
                Some(id) => id,
                None => {
                    error!(
                        "No spotify id found for artist with internal_id={} returned from \
                         averageing",
                        d.id
                    );
                    return false;
                },
            };
            let was_fetched = fetched_artists
                .iter()
                .any(|a| a.id == *avg_artist_spotify_id);
            if !was_fetched {
                error!(
                    "Failed to find artist metadata for artist with spotify_id={}",
                    avg_artist_spotify_id
                );
            }
            return was_fetched;
        });
        assert_eq!(fetched_artists.len(), average_artists.len());
    }

    let mut out_artists: Vec<AverageArtistItem> = average_artists
        .into_iter()
        .filter_map(|d| {
            let avg_artist_spotify_id = match artist_spotify_ids_by_internal_id.get(&(d.id as i32))
            {
                Some(id) => id,
                None => {
                    error!(
                        "No spotify id found for artist with internal_id={} returned from \
                         averageing",
                        d.id
                    );
                    return None;
                },
            };
            let artist = match fetched_artists
                .iter()
                .find(|artist| artist.id == *avg_artist_spotify_id)
                .cloned()
            {
                Some(artist) => artist,
                None => {
                    warn!(
                        "Didn't find artist with id={} in response from Spotify even though we \
                         requested it and counts lined up; they probably did the thing where they \
                         gave a different ID back than the one we requested, both of which refer \
                         to the same actual artist.",
                        avg_artist_spotify_id
                    );

                    return None;
                },
            };

            let mut top_tracks = top_tracks_by_artist_spotify_id
                .remove(avg_artist_spotify_id)
                .unwrap_or_default();
            // If the artist doesn't have any tracks, it's not worth showing to the user
            if top_tracks.is_empty() {
                return None;
            }

            // Put tracks without a preview URL at the end
            top_tracks.sort_by_key(|t| if t.preview_url.is_some() { 0 } else { 1 });
            // We don't really have space in the UI to show artists for every track, so we strip
            // them out here
            for track in &mut top_tracks {
                track.artists = Vec::new();
                track.album.artists = Vec::new();
            }

            Some(AverageArtistItem {
                artist,
                top_tracks,
                similarity_to_target_point: d.similarity_to_target_point,
                similarity_to_artist_1: d.similarity_to_artist_1,
                similarity_to_artist_2: d.similarity_to_artist_2,
            })
        })
        .collect();

    out_artists.sort_unstable_by_key(|item| Reverse(item.score()));

    let ctx = get_artist_embedding_ctx();

    endpoint_response_time("get_average_artists").observe(start.elapsed().as_nanos() as u64);

    Ok(Json(AverageArtistsResponse {
        artists: out_artists,
        distance: ctx
            .distance(artist_1_id as usize, artist_2_id as usize)
            .unwrap(),
        similarity: ctx
            .similarity(artist_1_id as usize, artist_2_id as usize)
            .unwrap(),
    }))
}

#[get("/artist_image_url/<artist_spotify_id>")]
pub(crate) async fn get_artist_image_url(
    artist_spotify_id: String,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<String, String> {
    let start = Instant::now();

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let artist: Option<Artist> = fetch_artists(&spotify_access_token, &[&artist_spotify_id])
        .await?
        .into_iter()
        .next();
    let image = match artist
        .and_then(|artist| artist.images.and_then(|images| images.into_iter().next()))
    {
        Some(image) => image,
        None => return Err(String::from("Not found")),
    };
    endpoint_response_time("get_artist_image_url").observe(start.elapsed().as_nanos() as u64);
    Ok(image.url)
}

#[post(
    "/refetch_cached_artists_missing_popularity?<count>",
    data = "<api_token_data>"
)]
pub(crate) async fn refetch_cached_artists_missing_popularity(
    api_token_data: rocket::Data<'_>,
    token_data: &State<Mutex<SpotifyTokenData>>,
    count: Option<usize>,
) -> Result<status::Custom<String>, String> {
    let start = Instant::now();
    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let mut redis_conn = spawn_blocking(|| get_redis_conn()).await.unwrap()?;

    let (mut redis_conn, artist_spotify_ids) =
        spawn_blocking(move || -> Result<(_, Vec<String>), String> {
            let artist_spotify_ids = redis::cmd("HRANDFIELD")
                .arg(&CONF.artists_cache_hash_name)
                .arg(count.unwrap_or(20).to_string())
                .query::<Vec<String>>(&mut *redis_conn)
                .map_err(|err| {
                    error!(
                        "Error getting random artist keys from Redis cache: {:?}",
                        err
                    );
                    String::from("Redis error")
                })?;
            Ok((redis_conn, artist_spotify_ids))
        })
        .await
        .unwrap()?;
    let artist_spotify_ids: Vec<&str> = artist_spotify_ids.iter().map(String::as_str).collect();
    let mut artists = fetch_artists(&spotify_access_token, &artist_spotify_ids).await?;
    artists.retain(|artist| artist.popularity.is_none());
    if artists.is_empty() {
        return Ok(status::Custom(Status::Ok, "No artists to refetch".into()));
    }
    let artist_ids_needing_refetch: Vec<String> =
        artists.iter().map(|artist| artist.id.clone()).collect();

    // Delete from the cache and then re-fetch them to re-populate the cache from the Spotify API
    let artist_ids_needing_refetch_clone = artist_ids_needing_refetch.clone();
    let deleted_artist_count = spawn_blocking(move || {
        let artist_ids_needing_refetch: Vec<&str> = artist_ids_needing_refetch_clone
            .iter()
            .map(String::as_str)
            .collect();

        let mut cmd = redis::cmd("HDEL");
        cmd.arg(&CONF.artists_cache_hash_name);
        for artist_id in artist_ids_needing_refetch {
            cmd.arg(artist_id);
        }
        cmd.query::<usize>(&mut *redis_conn)
    })
    .await
    .unwrap()
    .map_err(|err| {
        error!("Error deleting artist ids from Redis cache: {}", err);
        String::from("Redis error")
    })?;
    info!("Deleted {} artists from Redis cache", deleted_artist_count);

    let artist_ids_needing_refetch: Vec<&str> = artist_ids_needing_refetch
        .iter()
        .map(String::as_str)
        .collect();
    fetch_artists(&spotify_access_token, &artist_ids_needing_refetch).await?;

    endpoint_response_time("refetch_cached_artists_missing_popularity")
        .observe(start.elapsed().as_nanos() as u64);

    Ok(status::Custom(
        Status::Ok,
        format!(
            "Successfully fetched {} artists missing popularities",
            deleted_artist_count
        ),
    ))
}

/// Needed so that the MIME type on packed binary stuff that still should be compressed is picked up
/// by the CDN as being compressable.
#[derive(Responder)]
#[response(status = 200, content_type = "application/json")]
pub(crate) struct JSONMimeTypeSetterResponder {
    inner: Vec<u8>,
}

#[get("/packed_3d_artist_coords")]
pub(crate) async fn get_packed_3d_artist_coords_route(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
) -> Result<JSONMimeTypeSetterResponder, String> {
    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let packed = get_packed_3d_artist_coords(&conn, &spotify_access_token).await?;
    Ok(JSONMimeTypeSetterResponder {
        inner: packed.to_vec(),
    })
}

#[post("/map_artist_data_by_internal_ids", data = "<artist_internal_ids>")]
pub(crate) async fn get_artists_by_internal_ids(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    artist_internal_ids: Json<Vec<i32>>,
) -> Result<Json<Vec<Option<String>>>, String> {
    let start = Instant::now();

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let artist_internal_ids: Vec<i32> = artist_internal_ids.0;
    let artist_spotify_ids_by_internal_id =
        get_artist_spotify_ids_by_internal_id(&conn, artist_internal_ids.clone())
            .await
            .map_err(|err| {
                error!(
                    "Error getting artist spotify IDs by internal IDs: {:?}",
                    err
                );
                String::from("Internal DB error")
            })?;
    let artist_spotify_ids = artist_internal_ids
        .iter()
        .filter_map(|internal_id| {
            artist_spotify_ids_by_internal_id
                .get(internal_id)
                .map(String::as_str)
        })
        .collect::<Vec<_>>();

    let artists = fetch_artists(&spotify_access_token, &artist_spotify_ids).await?;
    let res = artist_internal_ids
        .into_iter()
        .map(|internal_id| {
            let spotify_id = artist_spotify_ids_by_internal_id.get(&internal_id)?;
            artists
                .iter()
                .find(|artist| artist.id == *spotify_id)
                .map(|artist| artist.name.clone())
        })
        .collect();

    endpoint_response_time("get_artists_by_internal_ids")
        .observe(start.elapsed().as_nanos() as u64);

    Ok(Json(res))
}

fn pack_artist_relationships(artist_relationships: Vec<Vec<i32>>) -> Vec<u8> {
    // Encoding:
    // artist count * u8: related artist count
    // 0-3 bytes of padding to make total byte count divisible by 4
    // The rest: u32s, in order, for each artist.
    let mut packed: Vec<u8> = Vec::new();
    for related_artists in &artist_relationships {
        let artist_count = related_artists.len();
        assert!(artist_count <= 255);
        packed.push(artist_count as u8);
    }

    // padding
    let padding_byte_count = 4 - (packed.len() % 4);
    for _ in 0..padding_byte_count {
        packed.push(0);
    }
    assert_eq!(packed.len() % 4, 0);

    for mut related_artists in artist_relationships {
        // Might help with compression ratio, who knows
        related_artists.sort_unstable();
        for id in related_artists {
            let bytes: [u8; 4] = unsafe { std::mem::transmute(id as u32) };
            for byte in bytes {
                packed.push(byte);
            }
        }
    }
    assert_eq!(packed.len() % 4, 0);
    packed
}

async fn get_packed_artist_relationships_by_internal_ids_inner(
    conn: &DbConn,
    spotify_access_token: String,
    artist_internal_ids: Vec<i32>,
) -> Result<Vec<u8>, String> {
    let tok = start();
    let artist_spotify_ids_by_internal_id =
        get_artist_spotify_ids_by_internal_id(&conn, artist_internal_ids.clone())
            .await
            .map_err(|err| {
                error!(
                    "Error getting artist spotify IDs by internal IDs: {:?}",
                    err
                );
                String::from("Internal DB error")
            })?;

    mark(tok, "Converted to spotify IDs");
    let artist_spotify_ids = artist_internal_ids
        .iter()
        .filter_map(|internal_id| {
            artist_spotify_ids_by_internal_id
                .get(internal_id)
                .map(String::as_str)
        })
        .collect::<Vec<_>>();

    let tok = start();
    let related_artists =
        get_multiple_related_artists(spotify_access_token, &artist_spotify_ids).await?;
    mark(tok, "Got related artists");
    assert_eq!(related_artists.len(), artist_spotify_ids.len());

    let tok = start();
    let related_artists_internal_ids_by_spotify_id = get_internal_ids_by_spotify_id(
        &conn,
        related_artists
            .iter()
            .flat_map(|related_artists| related_artists.iter()),
    )
    .await?;
    mark(tok, "Mapped back to internal IDs");

    let res = related_artists
        .into_iter()
        .map(|related_artists| {
            related_artists
                .iter()
                .filter_map(|artist_spotify_id| {
                    related_artists_internal_ids_by_spotify_id
                        .get(artist_spotify_id)
                        .copied()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    Ok(pack_artist_relationships(res))
}

#[post(
    "/map_artist_relationships_by_internal_ids",
    data = "<artist_internal_ids>"
)]
pub(crate) async fn get_packed_artist_relationships_by_internal_ids(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    artist_internal_ids: Json<Vec<i32>>,
) -> Result<JSONMimeTypeSetterResponder, String> {
    let start = Instant::now();

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let artist_internal_ids: Vec<i32> = artist_internal_ids.0;
    let packed = get_packed_artist_relationships_by_internal_ids_inner(
        &conn,
        spotify_access_token,
        artist_internal_ids,
    )
    .await?;
    endpoint_response_time("get_packed_artist_relationships_by_internal_ids")
        .observe(start.elapsed().as_nanos() as u64);
    Ok(JSONMimeTypeSetterResponder { inner: packed })
}

lazy_static::lazy_static! {
    pub static ref ARTIST_RELATIONSHIPS_BY_INTERNAL_IDS_CACHE:
        Arc<Mutex<HashMap<(u32, u32), Vec<u8>>>> =
            Arc::new(Mutex::new(HashMap::default()));
}

#[get("/map_artist_relationships_chunk?<chunk_size>&<chunk_ix>")]
pub(crate) async fn get_artist_relationships_chunk(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    chunk_size: u32,
    chunk_ix: u32,
) -> Result<JSONMimeTypeSetterResponder, String> {
    let start = Instant::now();

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let cache_key = (chunk_size, chunk_ix);
    {
        let cache = &mut *ARTIST_RELATIONSHIPS_BY_INTERNAL_IDS_CACHE.lock().await;
        if let Some(cached_data) = cache.get(&cache_key) {
            return Ok(JSONMimeTypeSetterResponder {
                inner: cached_data.clone(),
            });
        }
    }

    let artist_internal_ids: Vec<i32> = get_map_3d_artist_ctx(&conn, &spotify_access_token)
        .await
        .sorted_artist_ids
        .chunks(chunk_size as usize)
        .skip(chunk_ix as usize)
        .next()
        .unwrap_or_default()
        .iter()
        .copied()
        .map(|id| id as i32)
        .collect();

    let packed = get_packed_artist_relationships_by_internal_ids_inner(
        &conn,
        spotify_access_token,
        artist_internal_ids,
    )
    .await?;

    {
        let cache = &mut *ARTIST_RELATIONSHIPS_BY_INTERNAL_IDS_CACHE.lock().await;
        cache.insert(cache_key, packed.clone());
    }

    endpoint_response_time("get_artist_relationships_chunk")
        .observe(start.elapsed().as_nanos() as u64);

    Ok(JSONMimeTypeSetterResponder { inner: packed })
}

#[get("/get_preview_urls_by_internal_id/<artist_internal_id>")]
pub(crate) async fn get_preview_urls_by_internal_id(
    conn: DbConn,
    token_data: &State<Mutex<SpotifyTokenData>>,
    artist_internal_id: i32,
) -> Result<Json<Option<Vec<String>>>, String> {
    let start = Instant::now();

    let spotify_access_token = {
        let token_data = &mut *(&*token_data).lock().await;
        token_data.get().await
    }?;

    let spotify_ids_by_internal_id =
        get_artist_spotify_ids_by_internal_id(&conn, vec![artist_internal_id])
            .await
            .map_err(|err| {
                error!(
                    "Error getting artist spotify IDs by internal IDs: {:?}",
                    err
                );
                String::from("Internal DB error")
            })?;

    let spotify_id = match spotify_ids_by_internal_id.get(&artist_internal_id).cloned() {
        Some(spotify_id) => spotify_id,
        None => return Ok(Json(None)),
    };

    let top_tracks = fetch_top_tracks_for_artist(&spotify_access_token, &spotify_id).await?;

    endpoint_response_time("get_preview_urls_by_internal_id")
        .observe(start.elapsed().as_nanos() as u64);

    if top_tracks.is_empty() {
        return Ok(Json(None));
    }

    Ok(Json(
        top_tracks
            .iter()
            .map(|track| track.preview_url.clone())
            .collect(),
    ))
}

#[get("/top_artists_internal_ids_for_user/<user_id>")]
pub(crate) async fn get_top_artists_internal_ids_for_user(
    conn: DbConn,
    user_id: String,
) -> Result<Option<Json<Vec<i32>>>, String> {
    let start = Instant::now();

    let user = match db_util::get_user_by_spotify_id(&conn, user_id).await? {
        Some(user) => user,
        None => {
            return Ok(None);
        },
    };

    let top_artists = get_all_top_artists_for_user(&conn, user.id)
        .await
        .map_err(|err| {
            error!("Error getting top artists for user: {:?}", err);
            String::from("Internal DB error")
        })?;

    endpoint_response_time("get_top_artists_internal_ids_for_user")
        .observe(start.elapsed().as_nanos() as u64);

    Ok(Some(Json(
        top_artists
            .into_iter()
            .map(|(internal_id, _spotify_id)| internal_id)
            .collect(),
    )))
}

#[post(
    "/transfer_user_data_to_external_storage/<user_id>",
    data = "<api_token_data>"
)]
pub(crate) async fn transfer_user_data_to_external_storage(
    api_token_data: rocket::Data<'_>,
    conn: DbConn,
    user_id: String,
) -> Result<status::Custom<String>, String> {
    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    let user = match db_util::get_user_by_spotify_id(&conn, user_id).await? {
        Some(user) => user,
        None => {
            return Err(String::from("User not found"));
        },
    };

    if !user.external_data_retrieved {
        warn!(
            "User {} already has external user data stored; downloading + merging and re-storing \
             everything...",
            user.spotify_id
        );
    }

    if let Err(err) =
        crate::external_storage::upload::store_external_user_data(&conn, user.spotify_id).await
    {
        error!("Error storing external user data: {err}");
    }
    Ok(status::Custom(Status::Ok, String::new()))
}

#[post(
    "/transfer_user_data_from_external_storage/<user_id>",
    data = "<api_token_data>"
)]
pub(crate) async fn transfer_user_data_from_external_storage(
    api_token_data: rocket::Data<'_>,
    conn: DbConn,
    user_id: String,
) -> Result<status::Custom<String>, String> {
    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    let user = match db_util::get_user_by_spotify_id(&conn, user_id).await? {
        Some(user) => user,
        None => {
            return Err(String::from("User not found"));
        },
    };

    if user.external_data_retrieved {
        warn!(
            "User {} already has external data retrieved; retrieving anyway...",
            user.spotify_id
        );
    }

    crate::external_storage::download::retrieve_external_user_data(&conn, user.spotify_id, false)
        .await;
    Ok(status::Custom(Status::Ok, String::new()))
}

#[post(
    "/bulk_transfer_user_data_to_external_storage/<user_count>?<only_already_stored>&<concurrency>",
    data = "<api_token_data>"
)]
pub(crate) async fn bulk_transfer_user_data_to_external_storage(
    api_token_data: rocket::Data<'_>,
    conn0: DbConn,
    conn1: DbConn,
    conn2: DbConn,
    conn3: DbConn,
    conn4: DbConn,
    user_count: u32,
    only_already_stored: Option<bool>,
    concurrency: Option<usize>,
) -> Result<status::Custom<String>, String> {
    if !validate_api_token(api_token_data).await? {
        return Ok(status::Custom(
            Status::Unauthorized,
            "Invalid API token supplied".into(),
        ));
    }

    // Only transfer data for users that haven't viewed their profile in the past 4 months
    let cutoff_time: NaiveDateTime = Utc::now().naive_utc() - chrono::Duration::days(120);

    let users = conn0
        .run(move |conn| {
            use crate::schema::users;
            let mut query = users::table
                .filter(users::dsl::last_viewed.lt(cutoff_time))
                .into_boxed();
            if only_already_stored == Some(true) {
                query = query.filter(users::dsl::external_data_retrieved.eq(true));
            }

            query
                .order_by(users::dsl::last_external_data_store.asc())
                .limit(user_count as i64)
                .load::<User>(conn)
        })
        .await
        .map_err(|err| {
            error!("Error getting users from DB for bulk transfer: {err:?}");
            String::from("Internal DB error")
        })?;
    let usernames = users
        .iter()
        .map(|user| user.spotify_id.clone())
        .collect::<Vec<_>>();
    info!("Bulk transferring user data for {user_count} users: {usernames:?}");

    let concurrency = concurrency.unwrap_or(1).clamp(1, 5);
    let conns = Arc::new(Mutex::new(vec![conn0, conn1, conn2, conn3, conn4]));
    futures::stream::iter(users)
        .for_each_concurrent(Some(concurrency), |user| {
            let conns = Arc::clone(&conns);
            async move {
                if !user.external_data_retrieved {
                    warn!(
                        "User {} already has external user data stored; downloading + merging and \
                         re-storing everything...",
                        user.spotify_id
                    );
                }

                let conn = match conns.lock().await.pop() {
                    Some(conn) => conn,
                    None => {
                        error!("Shouldn't be possible; ran out of connections");
                        return;
                    },
                };

                match crate::external_storage::upload::store_external_user_data(
                    &conn,
                    user.spotify_id.clone(),
                )
                .await
                {
                    Ok(()) => info!("Successfully transferred user data for {}", user.spotify_id),
                    Err(err) => error!(
                        "Error transferring user data for {}: {err}",
                        user.spotify_id
                    ),
                }

                conns.lock().await.push(conn);
            }
        })
        .await;

    Ok(status::Custom(Status::Ok, String::new()))
}
