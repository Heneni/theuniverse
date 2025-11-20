# Static Spotifytrack Dashboard – Vercel Deployment

## 🚀 One-Click Deploy (Vercel)
1. **Export your `listening_history.csv` from Spotify**.
2. Run the following command from the root:
   ```bash
   node scripts/csv-to-json.js
   # This puts 'listening_history.json' into 'frontend/public/'
   ```
3. **Commit and push** all changes to GitHub.
4. **Go to [vercel.com/import](https://vercel.com/import)** and import your repository.
    - **Set "Root Directory"**: `frontend`
    - **Build Command**: `npm run build`
    - **Output Directory**: `build`
5. **Click "Deploy"** – Vercel will provide your live, auto-updating dashboard.
6. **To update:**  
   Repeat steps 1–3. Each push auto-updates your live Vercel site!

> No backend, no Codespaces, no servers—just update your CSV and push!