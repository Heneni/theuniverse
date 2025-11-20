#!/usr/bin/env python3
"""
Convert Spotify JSON streaming history to CSV format for Spotifytrack

Usage:
    python3 convert_spotify_json.py [input_dir] [output_file]
    
    input_dir: Directory containing Spotify JSON files (default: current directory)
    output_file: Output CSV file path (default: listening_history.csv)

Example:
    python3 convert_spotify_json.py ./spotify_data listening_history.csv
"""

import json
import csv
import sys
import os
from glob import glob
from datetime import datetime

def find_json_files(directory):
    """Find all Spotify JSON streaming history files."""
    patterns = [
        'StreamingHistory*.json',
        'Streaming_History*.json',
        'endsong_*.json',  # Extended streaming history format
    ]
    
    files = []
    for pattern in patterns:
        files.extend(glob(os.path.join(directory, pattern)))
    
    return sorted(files)

def parse_spotify_json(file_path):
    """Parse a Spotify JSON file and return list of streams."""
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            data = json.load(f)
            
        if not isinstance(data, list):
            print(f"Warning: {file_path} does not contain a list. Skipping.")
            return []
            
        return data
    except json.JSONDecodeError as e:
        print(f"Error parsing {file_path}: {e}")
        return []
    except Exception as e:
        print(f"Error reading {file_path}: {e}")
        return []

def extract_stream_data(stream):
    """Extract relevant data from a stream entry."""
    # Spotify uses different formats for different data exports
    # Handle multiple possible field names
    
    # Timestamp
    ts = (stream.get('endTime') or 
          stream.get('ts') or 
          stream.get('end_time') or
          datetime.now().isoformat())
    
    # Track name
    track_name = (stream.get('trackName') or
                  stream.get('master_metadata_track_name') or
                  stream.get('track_name') or
                  'Unknown Track')
    
    # Artist name
    artist_name = (stream.get('artistName') or
                   stream.get('master_metadata_album_artist_name') or
                   stream.get('artist_name') or
                   'Unknown Artist')
    
    # Milliseconds played
    ms_played = (stream.get('msPlayed') or
                 stream.get('ms_played') or
                 0)
    
    return {
        'ts': ts,
        'Track Name': track_name,
        'Artist Name(s)': artist_name,
        'ms_played': int(ms_played),
        'Genres': '',  # Spotify doesn't include genres in exports
        'Artist Genres': ''  # Spotify doesn't include genres in exports
    }

def convert_to_csv(input_dir, output_file):
    """Convert all Spotify JSON files in directory to CSV."""
    print(f"Searching for Spotify JSON files in: {input_dir}")
    
    json_files = find_json_files(input_dir)
    
    if not json_files:
        print("No Spotify JSON files found!")
        print("Looking for files matching: StreamingHistory*.json, Streaming_History*.json, endsong_*.json")
        return False
    
    print(f"Found {len(json_files)} file(s):")
    for f in json_files:
        print(f"  - {os.path.basename(f)}")
    
    all_streams = []
    for json_file in json_files:
        print(f"Processing {os.path.basename(json_file)}...")
        streams = parse_spotify_json(json_file)
        all_streams.extend(streams)
    
    if not all_streams:
        print("No streaming data found in JSON files!")
        return False
    
    print(f"\nTotal streams found: {len(all_streams)}")
    
    # Sort by timestamp
    all_streams.sort(key=lambda x: extract_stream_data(x)['ts'])
    
    # Write to CSV
    print(f"Writing to {output_file}...")
    with open(output_file, 'w', newline='', encoding='utf-8') as csvfile:
        fieldnames = ['ts', 'Track Name', 'Artist Name(s)', 'ms_played', 'Genres', 'Artist Genres']
        writer = csv.DictWriter(csvfile, fieldnames=fieldnames)
        
        writer.writeheader()
        for stream in all_streams:
            try:
                row = extract_stream_data(stream)
                writer.writerow(row)
            except Exception as e:
                print(f"Warning: Skipping invalid stream entry: {e}")
                continue
    
    print(f"\n✅ Success! Converted {len(all_streams)} streams to {output_file}")
    print(f"\nNext steps:")
    print(f"1. Move {output_file} to backend/listening_history.csv")
    print(f"2. Run: just dev")
    print(f"3. Open http://localhost:9050 in your browser")
    
    return True

def main():
    """Main entry point."""
    input_dir = sys.argv[1] if len(sys.argv) > 1 else '.'
    output_file = sys.argv[2] if len(sys.argv) > 2 else 'listening_history.csv'
    
    print("=" * 60)
    print("Spotify JSON to CSV Converter for Spotifytrack")
    print("=" * 60)
    print()
    
    if not os.path.isdir(input_dir):
        print(f"Error: Directory not found: {input_dir}")
        return 1
    
    success = convert_to_csv(input_dir, output_file)
    
    if not success:
        print("\n❌ Conversion failed!")
        print("\nMake sure you have:")
        print("1. Downloaded your Spotify data from https://www.spotify.com/account/privacy/")
        print("2. Extracted the ZIP file")
        print("3. Run this script in the directory containing the JSON files")
        return 1
    
    return 0

if __name__ == '__main__':
    sys.exit(main())
