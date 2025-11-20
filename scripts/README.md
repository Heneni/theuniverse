# Utility Scripts

This directory contains utility scripts for working with Spotifytrack.

## convert_spotify_json.py

Converts Spotify's JSON streaming history export to the CSV format required by Spotifytrack.

### Usage

```bash
python3 scripts/convert_spotify_json.py [input_directory] [output_file]
```

**Parameters:**
- `input_directory` (optional): Directory containing Spotify JSON files (default: current directory)
- `output_file` (optional): Output CSV file path (default: listening_history.csv)

### Examples

**Basic usage** (files in current directory):
```bash
python3 scripts/convert_spotify_json.py
```

**Specify input directory**:
```bash
python3 scripts/convert_spotify_json.py ~/Downloads/my_spotify_data
```

**Specify both input and output**:
```bash
python3 scripts/convert_spotify_json.py ./spotify_data backend/listening_history.csv
```

### Supported Spotify Export Formats

The script automatically detects and handles:
- `StreamingHistory*.json` - Standard streaming history
- `Streaming_History*.json` - Alternative naming
- `endsong_*.json` - Extended streaming history format

### Output

The script creates a CSV file with the following columns:
- `ts` - Timestamp (ISO 8601 format)
- `Track Name` - Name of the track
- `Artist Name(s)` - Artist name(s)
- `ms_played` - Milliseconds played
- `Genres` - Empty (Spotify doesn't export genres)
- `Artist Genres` - Empty (Spotify doesn't export genres)

### Next Steps

After converting:
1. Move the generated CSV to `backend/listening_history.csv`
2. Run `just dev` to start the application
3. Open http://localhost:9050 in your browser

### Requirements

- Python 3.6 or higher
- No additional packages required (uses standard library only)
