# Deployment Fix Notes

## Problem Summary
The deployment builds on Vercel and Netlify were failing because:
1. The frontend build requires a WASM module (`engine.js`) that is built from Rust source code
2. CI/CD platforms don't have the Rust/WASM toolchain installed by default
3. The Vercel configuration had an incorrect output path

## Solution Implemented
Created a stub system that allows builds to succeed without the compiled WASM module:

### Files Added/Modified:
1. **`frontend/src/musicGalaxy/WasmClient/engine.stub.ts`**
   - Stub implementations that throw helpful errors
   - Allows webpack to resolve the module during build

2. **`frontend/src/musicGalaxy/WasmClient/engine.ts`**
   - Shim file that exports stubs by default
   - Gets replaced when `just build-wasm-client` is run
   - Committed to git (not ignored)

3. **`frontend/src/musicGalaxy/WasmClient/WasmClient.worker.ts`**
   - Added null checks and error handling
   - Uses `ensureInitialized()` helper consistently
   - Gracefully handles missing WASM engine

4. **`frontend/src/musicGalaxy/WasmClient/.gitignore`**
   - Updated to allow stub files
   - Still ignores generated WASM artifacts (engine.js, engine_bg.wasm, etc.)

5. **`vercel.json`**
   - Simplified configuration
   - Fixed output directory path to `frontend/dist`

6. **`netlify.toml`**
   - Added build configuration
   - Set base directory to `frontend`
   - Added SPA redirect rules

## Build Verification
The frontend builds successfully locally:
```bash
cd frontend
yarn install
yarn build
# Output is in frontend/dist/
```

## How It Works
1. **Without WASM**: The `engine.ts` file exports stub functions
   - Build succeeds
   - Music Galaxy feature won't work but main app is fine
   - User sees helpful error message if they try to use Music Galaxy

2. **With WASM** (local development):
   - Run `just build-wasm-client` from frontend directory
   - This builds the Rust WASM module
   - Copies generated files (including engine.js) to WasmClient directory
   - engine.js replaces engine.ts functionality
   - Music Galaxy feature works fully

## Deployment Platform Notes

### Vercel
Configuration: `vercel.json`
```json
{
  "buildCommand": "cd frontend && yarn && yarn build",
  "outputDirectory": "frontend/dist"
}
```

**If still failing, check**:
- Project settings in Vercel dashboard
- Build command override in project settings
- Environment variables
- Node.js version compatibility

### Netlify
Configuration: `netlify.toml`
```toml
[build]
  base = "frontend"
  command = "yarn install && yarn build"
  publish = "dist"
```

**If still failing, check**:
- Site settings in Netlify dashboard
- Build command override in site settings
- Deploy context (branch deploys vs production)
- Node.js version in build image

## Optional: Enable Full WASM Support in CI/CD

If you want the Music Galaxy feature to work in deployments, you'll need to:

1. Install Rust toolchain in CI/CD
2. Add wasm32 target
3. Install wasm-bindgen-cli
4. Install wasm-opt
5. Run WASM build before frontend build

Example for GitHub Actions:
```yaml
- name: Setup Rust
  uses: actions-rs/toolchain@v1
  with:
    toolchain: stable
    target: wasm32-unknown-unknown

- name: Install wasm-bindgen-cli
  run: cargo install wasm-bindgen-cli

- name: Install wasm-opt
  run: |
    wget https://github.com/WebAssembly/binaryen/releases/download/version_116/binaryen-version_116-x86_64-linux.tar.gz
    tar -xzf binaryen-version_116-x86_64-linux.tar.gz
    sudo cp binaryen-version_116/bin/wasm-opt /usr/local/bin/

- name: Build WASM
  run: cd frontend && just build-wasm-client

- name: Build Frontend
  run: cd frontend && yarn build
```

## Troubleshooting

### Build fails with "Module not found: ./engine"
- Ensure engine.ts and engine.stub.ts are committed
- Check .gitignore doesn't exclude them

### Build succeeds but Music Galaxy doesn't work
- This is expected without WASM build
- Add UI message to inform users
- OR set up full WASM build in CI/CD

### Vercel/Netlify shows different error
- Check platform-specific logs in their dashboards
- Verify Node.js version compatibility
- Check for platform-specific configuration overrides

## Testing Locally

Test the stub system:
```bash
cd frontend
# Make sure engine.ts exists (should export from engine.stub)
cat src/musicGalaxy/WasmClient/engine.ts

# Clean and build
rm -rf dist node_modules
yarn install
yarn build

# Should succeed with warnings about large bundles
# Check that dist/ directory is created with HTML files
ls -la dist/
```

Test with full WASM:
```bash
cd frontend
just build-wasm-client  # Requires Rust toolchain
yarn build
# Should have engine.js, engine_bg.wasm in WasmClient directory
```
