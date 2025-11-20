#!/bin/bash
set -e

echo "🔧 Running post-create setup..."

# Install frontend dependencies
echo "📦 Installing frontend dependencies..."
cd /workspaces/theuniverse/frontend
yarn install
echo "✓ Frontend dependencies installed"

# Pre-compile backend dependencies (speeds up first run)
echo "🦀 Pre-compiling Rust dependencies..."
cd /workspaces/theuniverse/backend
cargo fetch
echo "✓ Rust dependencies fetched"

echo "✓ Post-create setup complete!"
echo ""
echo "🎉 Your development environment is ready!"
echo "   The application will start automatically..."
