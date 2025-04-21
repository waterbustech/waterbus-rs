# ---- Build stage with nightly ----
FROM rustlang/rust:nightly AS builder

# Install system dependencies for building
RUN apt-get update && apt-get install -y pkg-config libssl-dev cmake clang

WORKDIR /usr/src/app

# Copy manifest files and local dependencies first to cache
COPY Cargo.toml Cargo.lock ./
COPY typesense-client ./typesense-client
COPY webrtc-manager ./webrtc-manager
COPY certificates ./certificates

# Dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true
RUN rm -r src

# Copy the actual source code
COPY . .
RUN cargo build --release

# ---- Runtime stage ----
FROM debian:trixie-slim

# Install PostgreSQL client libraries and runtime dependencies
RUN apt-get update && \
    apt-get install -y libssl-dev ca-certificates libpq-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/app/target/release/waterbus-rs /usr/local/bin/waterbus

# Expose ports
EXPOSE 5998
EXPOSE 5998/udp
EXPOSE 19200-19250/udp

# Run the binary
CMD ["/usr/local/bin/waterbus"]
