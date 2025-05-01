# ---- Build stage with nightly ----
FROM rustlang/rust:nightly AS builder

# Install system dependencies for building, including GStreamer
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev cmake clang \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev gstreamer1.0-libav \
    gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly

WORKDIR /usr/src/app

# Copy manifest files first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Copy workspace member crates
COPY crates/typesense-client ./crates/typesense-client
COPY crates/webrtc-manager ./crates/webrtc-manager
COPY crates/egress-manager ./crates/egress-manager
COPY crates/waterbus-proto ./crates/waterbus-proto
COPY certificates ./certificates

# Copy the rest of the workspace (including src for signalling)
COPY . .

# Build the specific binary in the workspace
RUN cargo build --release --bin signalling

# ---- Runtime stage ----
FROM debian:trixie-slim

# Install runtime dependencies including GStreamer runtime libs only (not dev)
RUN apt-get update && apt-get install -y \
    libssl-dev ca-certificates libpq-dev \
    libgstreamer1.0-0 gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly gstreamer1.0-libav && \
    rm -rf /var/lib/apt/lists/*

# Copy the compiled binary
COPY --from=builder /usr/src/app/target/release/signalling /usr/local/bin/signalling

# Expose ports
EXPOSE 5998
EXPOSE 5998/udp
EXPOSE 19000-20000/udp

# Run the binary
CMD ["/usr/local/bin/signalling"]
