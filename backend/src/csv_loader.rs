use std::sync::Arc;

use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::models::{Artist, Track};

#[derive(Debug, Clone, Deserialize)]
struct CsvRecord {
    ts: String,
    #[serde(rename = "Track Name")]
    track_name: String,
    #[serde(rename = "Artist Name(s)")]
    artist_name: String,
    ms_played: u64,
    #[serde(rename = "Genres")]
    genres: String,
    #[serde(rename = "Artist Genres")]
    artist_genres: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListeningEntry {
    pub timestamp: DateTime<Utc>,
    pub track_name: String,
    pub artist_name: String,
    pub ms_played: u64,
    pub genres: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CsvData {
    pub entries: Vec<ListeningEntry>,
    pub artists: FnvHashMap<String, Artist>,
    pub tracks: FnvHashMap<String, Track>,
    pub top_artists_short: Vec<String>,
    pub top_artists_medium: Vec<String>,
    pub top_artists_long: Vec<String>,
    pub top_tracks_short: Vec<String>,
    pub top_tracks_medium: Vec<String>,
    pub top_tracks_long: Vec<String>,
    // New fields for CSV-only mode
    pub artist_first_seen: FnvHashMap<String, DateTime<Utc>>,
    pub track_first_seen: FnvHashMap<String, DateTime<Utc>>,
    pub genre_history: Vec<(DateTime<Utc>, FnvHashMap<String, f32>)>,
    pub artist_relationships: FnvHashMap<String, Vec<String>>,
}

lazy_static::lazy_static! {
    static ref CSV_DATA: RwLock<Option<Arc<CsvData>>> = RwLock::new(None);
}

fn parse_genres(genres_str: &str) -> Vec<String> {
    genres_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Load and parse the CSV file
pub async fn load_csv_data() -> Result<(), String> {
    let csv_path = std::path::Path::new("listening_history.csv");
    
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(csv_path)
        .map_err(|e| format!("Failed to open CSV file: {}", e))?;

    let mut entries = Vec::new();
    let mut artist_play_counts: FnvHashMap<String, u64> = FnvHashMap::default();
    let mut track_play_counts: FnvHashMap<(String, String), u64> = FnvHashMap::default();
    let mut artist_genres_map: FnvHashMap<String, Vec<String>> = FnvHashMap::default();

    for result in rdr.deserialize() {
        let record: CsvRecord = result.map_err(|e| format!("Failed to parse CSV record: {}", e))?;
        
        let timestamp = DateTime::parse_from_rfc3339(&record.ts)
            .map_err(|e| format!("Failed to parse timestamp: {}", e))?
            .with_timezone(&Utc);

        let genres = if !record.artist_genres.is_empty() {
            parse_genres(&record.artist_genres)
        } else {
            parse_genres(&record.genres)
        };

        entries.push(ListeningEntry {
            timestamp,
            track_name: record.track_name.clone(),
            artist_name: record.artist_name.clone(),
            ms_played: record.ms_played,
            genres: genres.clone(),
        });

        *artist_play_counts.entry(record.artist_name.clone()).or_insert(0) += record.ms_played;
        *track_play_counts
            .entry((record.track_name.clone(), record.artist_name.clone()))
            .or_insert(0) += record.ms_played;
        artist_genres_map.insert(record.artist_name.clone(), genres);
    }

    // Sort entries by timestamp
    entries.sort_by_key(|e| e.timestamp);

    // Calculate top artists and tracks
    let (top_artists_short, top_artists_medium, top_artists_long) =
        calculate_top_artists(&entries, &artist_play_counts);
    let (top_tracks_short, top_tracks_medium, top_tracks_long) =
        calculate_top_tracks(&entries, &track_play_counts);

    // Build artist and track metadata
    let artists = build_artists(&artist_play_counts, &artist_genres_map);
    let tracks = build_tracks(&track_play_counts);

    // Calculate first seen dates for timeline
    let (artist_first_seen, track_first_seen) = calculate_first_seen(&entries);
    
    // Calculate genre history
    let genre_history = calculate_genre_history(&entries);
    
    // Calculate artist relationships based on co-occurrence
    let artist_relationships = calculate_artist_relationships(&entries);

    let csv_data = CsvData {
        entries,
        artists,
        tracks,
        top_artists_short,
        top_artists_medium,
        top_artists_long,
        top_tracks_short,
        top_tracks_medium,
        top_tracks_long,
        artist_first_seen,
        track_first_seen,
        genre_history,
        artist_relationships,
    };

    info!("Successfully loaded CSV data with {} entries, {} artists, {} tracks", 
          csv_data.entries.len(), csv_data.artists.len(), csv_data.tracks.len());
    
    *CSV_DATA.write().await = Some(Arc::new(csv_data));
    Ok(())
}

/// Get a reference to the loaded CSV data
pub async fn get_csv_data() -> Option<Arc<CsvData>> {
    CSV_DATA.read().await.clone()
}

fn calculate_top_artists(
    entries: &[ListeningEntry],
    artist_play_counts: &FnvHashMap<String, u64>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    // Use the latest timestamp from the data instead of current time
    let latest_timestamp = entries.last().map(|e| e.timestamp).unwrap_or_else(Utc::now);
    let four_weeks_ago = latest_timestamp - chrono::Duration::weeks(4);
    let six_months_ago = latest_timestamp - chrono::Duration::days(180);

    let mut short_counts: FnvHashMap<String, u64> = FnvHashMap::default();
    let mut medium_counts: FnvHashMap<String, u64> = FnvHashMap::default();

    for entry in entries.iter().rev() {
        if entry.timestamp > four_weeks_ago {
            *short_counts.entry(entry.artist_name.clone()).or_insert(0) += entry.ms_played;
        }
        if entry.timestamp > six_months_ago {
            *medium_counts.entry(entry.artist_name.clone()).or_insert(0) += entry.ms_played;
        }
    }

    let top_short = get_top_n(&short_counts, 50);
    let top_medium = get_top_n(&medium_counts, 50);
    let top_long = get_top_n(artist_play_counts, 50);

    (top_short, top_medium, top_long)
}

fn calculate_top_tracks(
    entries: &[ListeningEntry],
    track_play_counts: &FnvHashMap<(String, String), u64>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    // Use the latest timestamp from the data instead of current time
    let latest_timestamp = entries.last().map(|e| e.timestamp).unwrap_or_else(Utc::now);
    let four_weeks_ago = latest_timestamp - chrono::Duration::weeks(4);
    let six_months_ago = latest_timestamp - chrono::Duration::days(180);

    let mut short_counts: FnvHashMap<(String, String), u64> = FnvHashMap::default();
    let mut medium_counts: FnvHashMap<(String, String), u64> = FnvHashMap::default();

    for entry in entries.iter().rev() {
        let key = (entry.track_name.clone(), entry.artist_name.clone());
        if entry.timestamp > four_weeks_ago {
            *short_counts.entry(key.clone()).or_insert(0) += entry.ms_played;
        }
        if entry.timestamp > six_months_ago {
            *medium_counts.entry(key.clone()).or_insert(0) += entry.ms_played;
        }
    }

    let top_short = get_top_n_tracks(&short_counts, 50);
    let top_medium = get_top_n_tracks(&medium_counts, 50);
    let top_long = get_top_n_tracks(track_play_counts, 50);

    (top_short, top_medium, top_long)
}

fn get_top_n(counts: &FnvHashMap<String, u64>, n: usize) -> Vec<String> {
    let mut sorted: Vec<_> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    sorted.iter().take(n).map(|(name, _)| (*name).clone()).collect()
}

fn get_top_n_tracks(counts: &FnvHashMap<(String, String), u64>, n: usize) -> Vec<String> {
    let mut sorted: Vec<_> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    sorted
        .iter()
        .take(n)
        .map(|((track, artist), _)| format!("{} - {}", track, artist))
        .collect()
}

fn build_artists(
    artist_play_counts: &FnvHashMap<String, u64>,
    artist_genres_map: &FnvHashMap<String, Vec<String>>,
) -> FnvHashMap<String, Artist> {
    let mut artists = FnvHashMap::default();
    
    for (artist_name, _) in artist_play_counts.iter() {
        let genres = artist_genres_map
            .get(artist_name)
            .cloned();
        
        // Create a fake Spotify ID based on the artist name
        let spotify_id = format!("csv_{}", artist_name.replace(' ', "_").to_lowercase());
        
        artists.insert(
            spotify_id.clone(),
            Artist {
                id: spotify_id,
                name: artist_name.clone(),
                genres,
                images: Some(vec![]),
                popularity: Some(50), // Default popularity
            },
        );
    }
    
    artists
}

fn build_tracks(track_play_counts: &FnvHashMap<(String, String), u64>) -> FnvHashMap<String, Track> {
    let mut tracks = FnvHashMap::default();
    
    for ((track_name, artist_name), _) in track_play_counts.iter() {
        // Create a fake Spotify ID based on track and artist name
        let spotify_id = format!(
            "csv_{}",
            format!("{}_{}", track_name, artist_name)
                .replace(' ', "_")
                .to_lowercase()
        );
        
        let artist_id = format!("csv_{}", artist_name.replace(' ', "_").to_lowercase());
        
        tracks.insert(
            spotify_id.clone(),
            Track {
                id: spotify_id,
                name: track_name.clone(),
                artists: vec![Artist {
                    id: artist_id,
                    name: artist_name.clone(),
                    genres: None,
                    images: Some(vec![]),
                    popularity: None,
                }],
                album: crate::models::Album {
                    id: "csv_unknown".to_string(),
                    name: "Unknown Album".to_string(),
                    artists: vec![],
                    images: vec![],
                },
                preview_url: None,
            },
        );
    }
    
    tracks
}

fn calculate_first_seen(
    entries: &[ListeningEntry],
) -> (FnvHashMap<String, DateTime<Utc>>, FnvHashMap<String, DateTime<Utc>>) {
    let mut artist_first_seen: FnvHashMap<String, DateTime<Utc>> = FnvHashMap::default();
    let mut track_first_seen: FnvHashMap<String, DateTime<Utc>> = FnvHashMap::default();

    for entry in entries {
        let artist_id = format!("csv_{}", entry.artist_name.replace(' ', "_").to_lowercase());
        let track_key = format!("{} - {}", entry.track_name, entry.artist_name);
        let track_id = format!("csv_{}", track_key.replace(' ', "_").to_lowercase());

        artist_first_seen
            .entry(artist_id)
            .or_insert(entry.timestamp);
        track_first_seen
            .entry(track_id)
            .or_insert(entry.timestamp);
    }

    (artist_first_seen, track_first_seen)
}

fn calculate_genre_history(entries: &[ListeningEntry]) -> Vec<(DateTime<Utc>, FnvHashMap<String, f32>)> {
    if entries.is_empty() {
        return Vec::new();
    }

    let mut history = Vec::new();
    let mut genre_counts: FnvHashMap<String, f32> = FnvHashMap::default();
    
    // Group by month
    let first_timestamp = entries.first().unwrap().timestamp;
    let mut current_month_start = first_timestamp
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc())
        .unwrap();
    
    for entry in entries {
        // Check if we've moved to a new month
        let entry_month_start = entry.timestamp
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc())
            .unwrap();
        
        if entry_month_start > current_month_start + chrono::Duration::days(30) {
            // Save current month's data
            if !genre_counts.is_empty() {
                history.push((current_month_start, genre_counts.clone()));
            }
            current_month_start = entry_month_start;
            genre_counts.clear();
        }
        
        // Accumulate genre counts for this month
        for genre in &entry.genres {
            *genre_counts.entry(genre.clone()).or_insert(0.0) += entry.ms_played as f32;
        }
    }
    
    // Add final month
    if !genre_counts.is_empty() {
        history.push((current_month_start, genre_counts));
    }
    
    history
}

fn calculate_artist_relationships(entries: &[ListeningEntry]) -> FnvHashMap<String, Vec<String>> {
    let mut relationships: FnvHashMap<String, FnvHashMap<String, u32>> = FnvHashMap::default();
    
    // Use a sliding window to find artists that are listened to close together
    let window_size = 50; // Consider artists within 50 songs as related
    
    for window_start in 0..entries.len() {
        let window_end = (window_start + window_size).min(entries.len());
        let window = &entries[window_start..window_end];
        
        // Get unique artists in this window
        let mut artists_in_window: FnvHashMap<String, bool> = FnvHashMap::default();
        for entry in window {
            let artist_id = format!("csv_{}", entry.artist_name.replace(' ', "_").to_lowercase());
            artists_in_window.insert(artist_id, true);
        }
        
        // For each pair of artists in the window, increase their relationship score
        let artists: Vec<String> = artists_in_window.keys().cloned().collect();
        for i in 0..artists.len() {
            for j in (i + 1)..artists.len() {
                let artist1 = &artists[i];
                let artist2 = &artists[j];
                
                *relationships
                    .entry(artist1.clone())
                    .or_default()
                    .entry(artist2.clone())
                    .or_insert(0) += 1;
                *relationships
                    .entry(artist2.clone())
                    .or_default()
                    .entry(artist1.clone())
                    .or_insert(0) += 1;
            }
        }
    }
    
    // Convert to final format: for each artist, return top N related artists
    let mut final_relationships: FnvHashMap<String, Vec<String>> = FnvHashMap::default();
    for (artist, related) in relationships {
        let mut related_vec: Vec<(String, u32)> = related.into_iter().collect();
        related_vec.sort_by(|a, b| b.1.cmp(&a.1));
        final_relationships.insert(
            artist,
            related_vec.iter().take(20).map(|(id, _)| id.clone()).collect(),
        );
    }
    
    final_relationships
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_csv_loader() {
        // Test loading CSV data
        let result = load_csv_data().await;
        assert!(result.is_ok(), "CSV loading should succeed");

        // Test getting loaded data
        let data = get_csv_data().await;
        assert!(data.is_some(), "CSV data should be loaded");

        let data = data.unwrap();
        assert!(!data.entries.is_empty(), "Should have listening entries");
        assert!(!data.artists.is_empty(), "Should have artists");
        assert!(!data.tracks.is_empty(), "Should have tracks");
        
        println!("Loaded {} entries", data.entries.len());
        println!("Loaded {} artists", data.artists.len());
        println!("Loaded {} tracks", data.tracks.len());
        println!("Top artists (short): {}", data.top_artists_short.len());
        println!("Top tracks (short): {}", data.top_tracks_short.len());
    }
}
