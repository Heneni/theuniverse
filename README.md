# Spotifytrack

**Spotifytrack** is a web application that provides a record of your listening habits on Spotify, allowing you to see how your preferences change over time and remember when you discovered your favorite tracks and artists.

It also includes some other Spotify-related tools like the [Artist Averager](https://spotifytrack.net/artist-averager.html).

Try it yourself: <https://spotifytrack.net>

![A screenshot of Spotifytrack showing the homepage for a user with a timeline showing recently discovered tracks and artists](https://i.ameo.link/98s.png)

## 🎵 CSV-Only Mode (Recommended for Personal Use)

**NEW!** Spotifytrack now supports a **CSV-only mode** that works entirely with your exported Spotify listening history - no API keys, no database setup, no OAuth required!

### Quick Start with Your Data

1. **Export your Spotify listening history**:
   - Go to your [Spotify Privacy Settings](https://www.spotify.com/account/privacy/)
   - Request your extended streaming history (takes a few days to receive)
   - Download and extract the files - you'll get multiple JSON files

2. **Convert to CSV** (if needed):
   - The application expects a `listening_history.csv` file with these columns:
     - `ts` - ISO 8601 timestamp (e.g., "2023-01-15T14:30:00Z")
     - `Track Name` - name of the track
     - `Artist Name(s)` - artist name
     - `ms_played` - milliseconds played
     - `Genres` - comma-separated list of genres (optional)
     - `Artist Genres` - comma-separated list of artist genres (optional)

3. **Place your CSV**:
   ```bash
   cp your_listening_history.csv backend/listening_history.csv
   ```

4. **Run the application**:
   ```bash
   just dev
   ```

5. **Open your browser** and go to `http://localhost:9050`

**That's it!** The application will:
- ✅ Load your listening history from CSV
- ✅ Calculate top artists, tracks, and genres
- ✅ Generate timeline events (first time you heard artists/tracks)
- ✅ Build artist relationship graphs based on your listening patterns
- ✅ Provide a fully functional dashboard with all features

### Features Available in CSV Mode

All main features work with just your CSV file:
- 📊 **Stats Dashboard**: Top artists, tracks, and genres across different time periods
- 📅 **Timeline View**: See when you first discovered artists and tracks
- 🎨 **Genre History**: Track how your music tastes evolve over time
- 🔗 **Artist Relationships**: Visual graph of related artists based on your listening
- 🔍 **Search**: Search through your listened artists
- 📈 **Artist Stats**: Detailed stats for individual artists

## Directories

 * `frontend` contains the entire web UI for Spotifytrack.  It is built with TypeScript + React.
 * `backend` contains the backend API server that furnishes all of the data for the web frontend using CSV data.
 * `research` contains Python notebooks used to generate, process, and analyze artist relationship data in order to generate artist embeddings for the artist averager.

## Building + Developing

### 🚀 Quick Start with GitHub Codespaces

The easiest way to try Spotifytrack is using GitHub Codespaces - no local setup required!

1. Click the **Code** button at the top of this repository
2. Select the **Codespaces** tab
3. Click **Create codespace on main** (or your preferred branch)
4. Upload your `listening_history.csv` to `backend/listening_history.csv`
5. Wait 2-3 minutes for the environment to initialize
6. Run `just dev` in the terminal
7. The application will start:
   - **Frontend**: Opens automatically in your browser on port 9050
   - **Backend API**: Running on port 8000

**That's it!** You now have a fully functional development environment with:
- ✅ All dependencies pre-installed (Rust, Node.js, yarn, just)
- ✅ Both frontend and backend running with hot-reload
- ✅ Your personal listening data loaded and ready

**Useful tips:**
- Stop services: Press `Ctrl+C` in the terminal
- Restart services: Run `just dev` in a new terminal
- Update your data: Replace `backend/listening_history.csv` and restart
- All logs are visible directly in the terminal output

### 💻 Local Development

Almost all tasks involved with building or running the code can be found in `Justfile`s throughout the project.  They can be run using the [just command runner](https://github.com/casey/just).

![A screenshot of Spotifytrack showing the artist relationship graph, an interactive visualization of the relationship between a user's top artists](https://ameo.link/u/98t.png)
