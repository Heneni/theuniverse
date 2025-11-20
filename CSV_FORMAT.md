# CSV Format Documentation

This document describes the expected format for the `listening_history.csv` file used by Spotifytrack's CSV-only mode.

## File Location

Place your CSV file at: `backend/listening_history.csv`

## CSV Format

The CSV file should have the following columns (with header row):

| Column | Type | Required | Description | Example |
|--------|------|----------|-------------|---------|
| `ts` | ISO 8601 DateTime | Yes | Timestamp when the track was played | `2023-01-15T14:30:00Z` |
| `Track Name` | String | Yes | Name of the track | `Bohemian Rhapsody` |
| `Artist Name(s)` | String | Yes | Name of the artist(s) | `Queen` |
| `ms_played` | Integer | Yes | Milliseconds the track was played | `354000` |
| `Genres` | String | No | Comma-separated list of genres | `rock, classic rock` |
| `Artist Genres` | String | No | Comma-separated artist genres | `rock, classic rock, british rock` |

## Example CSV

```csv
ts,Track Name,Artist Name(s),ms_played,Genres,Artist Genres
2023-01-15T14:30:00Z,Bohemian Rhapsody,Queen,354000,"rock, classic rock","rock,classic rock,british rock"
2023-01-15T15:00:00Z,Stairway to Heaven,Led Zeppelin,482000,"rock, hard rock","rock,hard rock,blues rock"
2023-01-15T15:15:00Z,Hotel California,Eagles,390000,"rock, soft rock","rock,soft rock,country rock"
```

## Getting Your Data from Spotify

### Method 1: Official Spotify Data Export (Recommended)

1. Go to your [Spotify Privacy Settings](https://www.spotify.com/account/privacy/)
2. Scroll down to "Download your data"
3. Request "Extended streaming history"
4. Wait for Spotify to prepare your data (can take up to 30 days)
5. Download the ZIP file when ready
6. Extract and you'll find JSON files with your listening history

### Method 2: Convert Spotify JSON to CSV

If you receive JSON files from Spotify, you can convert them to CSV format using Python:

```python
import json
import csv
from datetime import datetime

# Load Spotify JSON files
all_streams = []
for i in range(10):  # Adjust based on number of files
    try:
        with open(f'StreamingHistory{i}.json', 'r') as f:
            all_streams.extend(json.load(f))
    except FileNotFoundError:
        break

# Write to CSV
with open('listening_history.csv', 'w', newline='', encoding='utf-8') as csvfile:
    writer = csv.DictWriter(csvfile, fieldnames=[
        'ts', 'Track Name', 'Artist Name(s)', 'ms_played', 'Genres', 'Artist Genres'
    ])
    
    writer.writeheader()
    for stream in all_streams:
        # Spotify JSON format varies, adjust field names as needed
        writer.writerow({
            'ts': stream.get('endTime') or stream.get('ts', ''),
            'Track Name': stream.get('trackName') or stream.get('master_metadata_track_name', ''),
            'Artist Name(s)': stream.get('artistName') or stream.get('master_metadata_album_artist_name', ''),
            'ms_played': stream.get('msPlayed', 0),
            'Genres': '',  # Not provided by Spotify export
            'Artist Genres': ''  # Not provided by Spotify export
        })

print(f"Converted {len(all_streams)} streams to CSV")
```

## Data Requirements

- **Minimum rows**: At least 100 entries recommended for meaningful statistics
- **Time range**: Data can span any time period (weeks to years)
- **Order**: Rows should be in chronological order (oldest first), though the app will sort them
- **Duplicates**: Duplicate entries are handled gracefully

## Genre Information

While genres are optional, including them enriches the experience:
- **Without genres**: The app still works perfectly, showing artist and track statistics
- **With genres**: Enables genre history tracking and genre-specific artist stats

To add genres if missing:
- Use the Spotify Web API to fetch artist genres
- Use MusicBrainz or Last.fm APIs
- Manually add based on your knowledge

## Updating Your Data

To update your listening history:

1. Replace `backend/listening_history.csv` with your new file
2. Restart the application:
   ```bash
   # Stop the app (Ctrl+C)
   just dev
   ```
3. The backend will automatically reload and reprocess the new CSV

## Troubleshooting

### "CSV data not loaded" error
- Check that `listening_history.csv` exists in the `backend/` directory
- Verify the CSV has the required headers
- Check for CSV formatting errors (e.g., unescaped commas, quotes)

### Missing or incorrect stats
- Ensure timestamps are in valid ISO 8601 format
- Check that `ms_played` values are positive integers
- Verify artist and track names are non-empty

### Performance with large files
- Files up to 100,000 entries should load quickly (< 10 seconds)
- Very large files (> 500,000 entries) may take 30-60 seconds to process
- Consider filtering to recent years if processing takes too long

## Privacy

- Your CSV data stays completely local
- No data is sent to any external servers
- No API keys or authentication required
- Perfect for analyzing your listening history privately
