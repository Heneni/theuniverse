const fs = require("fs");
const path = require("path");
const csv = require("csv-parser");

const input = process.argv[2] || "listening_history.csv";
const output = process.argv[3] || "public/listening_history.json";

const results = [];

fs.createReadStream(input)
  .pipe(csv())
  .on("data", (data) => {
    // Map/convert here as needed (add extra fields, fix data types, etc)
    results.push({
      ts: data["ts"] || data["endTime"] || "", // try other headers too
      track_name: data["Track Name"] || data["trackName"] || "",
      artist_name: data["Artist Name(s)"] || data["artistName"] || "",
      ms_played: Number(data["ms_played"] || data["msPlayed"] || 0),
      album_name: data["Album Name"] || data["albumName"] || "",
      album_art_url: data["Album Art URL"] || "",
      track_id: data["Track ID"] || "",
      artist_id: data["Artist ID"] || "",
      genres: (data["Genres"] || data["Artist Genres"] || "")
        .split(",")
        .map((g) => g.trim())
        .filter(Boolean),
      // add more mappings if needed
    });
  })
  .on("end", () => {
    fs.writeFileSync(output, JSON.stringify(results, null, 2));
    console.log(`Converted CSV to JSON.\n${results.length} rows.`);
    console.log(`Saved to ${output}`);
  });
