#!/bin/bash

echo "🚀 Starting Spotifytrack development environment..."
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  WELCOME TO SPOTIFYTRACK CODESPACES!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "✨ Your development environment is starting automatically!"
echo ""
echo "📍 What's happening:"
echo "   • Backend API starting on port 8000"
echo "   • Frontend dev server starting on port 9050"
echo "   • MySQL database starting on port 3307"
echo ""
echo "🌐 After initialization (1-2 minutes):"
echo "   • Your browser will open with the frontend"
echo "   • Both services will be running and hot-reloading"
echo ""
echo "⚠️  Note: First launch may take 2-3 minutes while Rust compiles"
echo ""
echo "📝 Useful commands:"
echo "   • View logs: Check the terminal below"
echo "   • Restart: Run 'just dev' in the terminal"
echo "   • Stop: Press Ctrl+C in the terminal"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Start the dev environment in the background
cd /workspaces/theuniverse
nohup just dev > /tmp/dev.log 2>&1 &

echo "✓ Development servers starting in background..."
echo "  To view logs: tail -f /tmp/dev.log"
echo ""
