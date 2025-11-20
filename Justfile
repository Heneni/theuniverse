# Start the entire application stack for local development
# Runs backend (Rust/Rocket) and frontend (React) concurrently
dev:
  #!/usr/bin/env bash
  set -euo pipefail
  
  # Check for required dependencies
  echo "🔍 Checking for required dependencies..."
  
  if ! command -v cargo &> /dev/null; then
    echo "❌ Error: cargo is not installed. Please install Rust from https://rustup.rs/"
    exit 1
  fi
  echo "✓ cargo found"
  
  if ! command -v yarn &> /dev/null && ! command -v npm &> /dev/null; then
    echo "❌ Error: Neither yarn nor npm is installed. Please install Node.js from https://nodejs.org/"
    exit 1
  fi
  
  if command -v yarn &> /dev/null; then
    echo "✓ yarn found"
    PACKAGE_MANAGER="yarn"
  else
    echo "✓ npm found"
    PACKAGE_MANAGER="npm"
  fi
  
  echo ""
  echo "🚀 Starting Spotifytrack development environment..."
  echo ""
  echo "📦 Backend will run on:  http://localhost:8000"
  echo "🌐 Frontend will run on: http://localhost:9050"
  echo ""
  echo "📝 Note: The application uses CSV-based data from backend/listening_history.csv"
  echo ""
  echo "⚠️  To stop all services: Press Ctrl+C"
  echo ""
  
  # Create minimal .env file for local development if it doesn't exist
  if [ ! -f backend/.env ]; then
    echo "📝 Creating minimal .env file for local development..."
    {
      echo "# Minimal configuration for local CSV-based development"
      echo "# These are dummy values for local testing with CSV data"
      echo "# Note: A minimal MySQL container is started for compatibility, but routes use CSV data"
      echo "SPOTIFY_CLIENT_ID=\"dummy_client_id_for_local_dev\""
      echo "SPOTIFY_CLIENT_SECRET=\"dummy_client_secret_for_local_dev\""
      echo "API_SERVER_URL=\"http://localhost:8000\""
      echo "WEBSITE_URL=\"http://localhost:9050\""
      echo "REDIS_URL=\"redis://localhost:6379\""
      echo "ADMIN_API_TOKEN=\"local_dev_token\""
      echo "TELEMETRY_SERVER_PORT=\"4101\""
      echo "ROCKET_DATABASES=\"{ spotify_homepage = { url = \\\"mysql://spotifytrack:spotifytrack@localhost:3307/spotifytrack\\\" } }\""
    } > backend/.env
    echo "✓ Created backend/.env with dummy values for local development"
  fi
  
  # Start a minimal MySQL container for backend compatibility (routes use CSV data)
  echo "🗄️  Starting minimal MySQL container..."
  docker rm -f spotifytrack-mysql 2>/dev/null || true
  docker run -d \
    --name spotifytrack-mysql \
    -e MYSQL_ROOT_PASSWORD=root \
    -e MYSQL_DATABASE=spotifytrack \
    -e MYSQL_USER=spotifytrack \
    -e MYSQL_PASSWORD=spotifytrack \
    -p 3307:3306 \
    mysql:8.0 \
    --default-authentication-plugin=mysql_native_password 2>&1 | head -5
  
  # Wait for MySQL to be ready
  echo "⏳ Waiting for MySQL to be ready..."
  for i in {1..30}; do
    if docker exec spotifytrack-mysql mysqladmin ping -h localhost --silent 2>/dev/null; then
      echo "✓ MySQL is ready"
      break
    fi
    sleep 1
  done
  
  # Create a temporary file to track the backend PID and cleanup
  BACKEND_PID_FILE=$(mktemp)
  cleanup() {
    echo ''
    echo '🛑 Shutting down services...'
    kill $(cat $BACKEND_PID_FILE) 2>/dev/null || true
    docker rm -f spotifytrack-mysql 2>/dev/null || true
    rm -f $BACKEND_PID_FILE
    echo '✓ Services stopped'
    exit
  }
  trap cleanup INT TERM EXIT
  
  # Start the backend in the background
  echo "🔧 Starting backend (Rust/Rocket)..."
  cd backend
  RUST_LOG=info ROCKET_LOG_LEVEL=normal RUST_BACKTRACE=1 cargo run &
  BACKEND_PID=$!
  echo $BACKEND_PID > $BACKEND_PID_FILE
  cd ..
  
  # Give the backend a moment to start
  sleep 2
  
  # Check if backend is still running
  if ! kill -0 $BACKEND_PID 2>/dev/null; then
    echo "❌ Backend failed to start. Check the logs above for errors."
    exit 1
  fi
  
  echo "✓ Backend started (PID: $BACKEND_PID)"
  echo ""
  
  # Install frontend dependencies if needed
  cd frontend
  if [ ! -d "node_modules" ]; then
    echo "📦 Installing frontend dependencies..."
    $PACKAGE_MANAGER install
    echo "✓ Frontend dependencies installed"
    echo ""
  fi
  
  # Start the frontend in the foreground
  echo "⚛️  Starting frontend (React)..."
  REACT_APP_API_BASE_URL=http://localhost:8000 REACT_APP_SITE_URL=http://localhost:9050 $PACKAGE_MANAGER start --host 0.0.0.0 --port 9050

build-and-deploy:
  cd frontend && yarn build
  cd -

  cd backend && just docker-build
  cd -

  docker tag ameo/spotifytrack-backend:latest gcr.io/free-tier-164405/spotifytrack-backend:latest
  docker push gcr.io/free-tier-164405/spotifytrack-backend:latest

  gcloud config set run/region us-west1
  gcloud beta run deploy spotifytrack-backend \
    --platform managed \
    --set-env-vars="ROCKET_DATABASES=$ROCKET_DATABASES,\
      SPOTIFY_CLIENT_ID=$SPOTIFY_CLIENT_ID,\
      SPOTIFY_CLIENT_SECRET=$SPOTIFY_CLIENT_SECRET,\
      API_SERVER_URL=https://spotifytrack.net/api,\
      WEBSITE_URL=https://spotifytrack.net,\
      REDIS_URL=$REDIS_URL,\
      ADMIN_API_TOKEN=$ADMIN_API_TOKEN"\
    --image gcr.io/free-tier-164405/spotifytrack-backend:latest

  rsync -Prv -e "ssh -i ~/.ssh/id_rsa -o StrictHostKeyChecking=no -o IdentitiesOnly=yes -F /dev/null" ./frontend/dist/* root@spotifytrack.net:/var/www/spotifytrack
