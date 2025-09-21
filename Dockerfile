# WARNING: The COPY and RUN in the builder step is very important
# Always run 'cargo chef cook' on the recipe before copying src/
# Got errors indicating NixOS linkers were the problem in 'strace' due to immediate binary file exit

# Step 1: Create docker container that runs debian 12 specifically designed to run rust version 1.86
FROM rust:1.86.0-slim-bookworm AS chef
# Update preconfigured rust tools, install needed building, linkers, optimise, and remove package cache
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    lld \
    clang \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef

# Step 2: Optimise external dependency creation and create final binary
# Create temporary container which copies external dependencies from outside Dockerfile and creates recipe for caching
FROM chef AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Create a another temp container that copies the recipe from chef container, compiles external libs to .rlb files, copies src
# From outside the Dockerfile and builds the binary
FROM chef AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy the src/ dir and Cargo.toml from outside the Dockerfile and compile final binary
COPY src ./src
COPY Cargo.toml ./Cargo.toml
RUN cargo build --release --bin websocket-chat-server

# Step 3: Create final container and install needed packages to run the binary
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Install runtime dependencies
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# Copy the built application from the builder stage
COPY --from=builder /app/target/release/websocket-chat-server ./websocket-chat-server

# Set environment variables for better error reporting
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

CMD ["./websocket-chat-server"]
