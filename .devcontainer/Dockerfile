# Use the official Microsoft devcontainer base with Rust
FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Install Node.js 20.x (LTS)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs

# Install yarn globally
RUN npm install -g yarn

# Install just command runner
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

# Install additional dependencies for the backend
RUN apt-get update && apt-get install -y \
    libmariadbclient-dev-compat \
    libssl-dev \
    pkg-config \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Switch to vscode user
USER vscode

# Install Rust nightly toolchain (required by the project)
RUN rustup toolchain install nightly \
    && rustup default nightly

# Install wasm32 target for WASM features (optional but nice to have)
RUN rustup target add wasm32-unknown-unknown

# Install wasm-bindgen-cli for WASM client builds
RUN cargo install wasm-bindgen-cli

# Set working directory
WORKDIR /workspaces/theuniverse

# Keep container running
CMD ["sleep", "infinity"]
