# Vercel Deployment Troubleshooting

## Current Status
- ✅ **Netlify**: Successfully deployed - https://deploy-preview-8--spotstuff.netlify.app
- ❌ **Vercel**: Failing - requires manual investigation

## Why Netlify Works But Vercel Doesn't

The builds work locally and on Netlify, which proves the code changes are correct. Vercel's failure is likely due to platform-specific configuration issues.

## Steps to Fix Vercel

### 1. Access the Vercel Dashboard
Go to: https://vercel.com/maahks-projects/theuniverse

### 2. Check Build Logs
Look for the specific error in the deployment logs. Common errors include:

#### Error: Framework Detection Failed
**Solution**: Set framework to "Other" in project settings

#### Error: Build Command Not Found
**Solution**: Verify build command in project settings matches vercel.json

#### Error: Output Directory Not Found
**Solution**: Check that outputDirectory setting is correct

#### Error: Node.js Version Mismatch
**Solution**: Set Node.js version in project settings (try 18.x or 20.x)

### 3. Common Configuration Issues

#### Issue: Project Settings Override
Vercel project settings can override vercel.json. Check:
- Settings > General > Build & Development Settings
- Ensure "Override" toggle is OFF for build command
- OR ensure override matches: `cd frontend && yarn && yarn build`
- Output directory should be: `frontend/dist`

#### Issue: Framework Detection
Vercel might be trying to auto-detect the framework incorrectly:
- Go to Settings > General
- Set "Framework Preset" to "Other" or blank
- This forces Vercel to use vercel.json configuration

#### Issue: Root Directory
If Vercel is trying to build from root:
- Settings > General > Root Directory
- Should be empty (builds from root, but commands navigate to frontend)
- OR set to `frontend` and update vercel.json buildCommand

### 4. Recommended Vercel Configuration

Current `vercel.json`:
```json
{
  "buildCommand": "cd frontend && yarn && yarn build",
  "outputDirectory": "frontend/dist"
}
```

**Alternative 1** - If framework detection is causing issues:
```json
{
  "framework": null,
  "buildCommand": "cd frontend && yarn install && yarn build",
  "outputDirectory": "frontend/dist"
}
```

**Alternative 2** - Using root directory setting:
In Vercel dashboard, set Root Directory to `frontend`, then update vercel.json:
```json
{
  "buildCommand": "yarn install && yarn build",
  "outputDirectory": "dist"
}
```

**Alternative 3** - Using legacy v2 format:
```json
{
  "version": 2,
  "builds": [
    {
      "src": "frontend/package.json",
      "use": "@vercel/static-build",
      "config": {
        "distDir": "dist"
      }
    }
  ]
}
```

### 5. Environment Variables
Check if any environment variables are needed:
- Settings > Environment Variables
- Variables starting with `REACT_APP_` are baked into the build
- Current build command already sets necessary variables

### 6. Node.js Version
If build fails with Node.js errors:
- Settings > General > Node.js Version
- Try 18.x or 20.x
- Or add `.node-version` file to repo:
  ```
  18
  ```

### 7. Build Timeout
If builds are timing out:
- Free tier has shorter timeouts
- Check if build completes locally in under 10 minutes
- Consider upgrading plan if needed

## Testing Changes

After making configuration changes in Vercel dashboard:
1. Trigger a new deployment (push a small change or click "Redeploy")
2. Watch the build logs in real-time
3. Look for the specific error that occurs
4. Use the error message to identify which configuration issue to fix

## What We Know Works

- ✅ Local build completes successfully
- ✅ Netlify deploys successfully with similar configuration
- ✅ Frontend outputs to `frontend/dist` correctly
- ✅ All build commands execute properly
- ✅ WASM stub system works (proven by Netlify)

This means the code is correct - it's purely a Vercel configuration issue.

## If All Else Fails

### Nuclear Option: Delete and Reconnect
1. Delete the Vercel project
2. Create a new Vercel project
3. Connect to the same GitHub repo
4. Let Vercel auto-detect settings
5. If auto-detection fails, manually configure as shown above

### Contact Vercel Support
If configuration changes don't work:
1. Go to Vercel dashboard
2. Click "Help" or "Support"
3. Provide:
   - Link to failing deployment
   - Note that Netlify works fine
   - Share the vercel.json configuration
   - Ask for help with framework detection or build command issues

## Success Criteria

Vercel deployment will be successful when you see:
- ✅ Build completes without errors
- ✅ Output directory contains HTML, JS, CSS files
- ✅ Preview URL loads the application
- ✅ No 404 errors on page navigation

The application will work the same as Netlify preview since the code is identical.
