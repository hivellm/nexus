# Multi-stage Dockerfile for Nexus Graph Database
# Build stage
FROM rustlang/rust:nightly AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY nexus-core/Cargo.toml ./nexus-core/
COPY nexus-server/Cargo.toml ./nexus-server/
COPY nexus-protocol/Cargo.toml ./nexus-protocol/

# Copy source code
COPY nexus-core ./nexus-core
COPY nexus-server ./nexus-server
COPY nexus-protocol ./nexus-protocol

# Build in release mode
RUN cargo +nightly build --release --workspace

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 nexus && \
    mkdir -p /app/data /app/config /run/secrets && \
    chown -R nexus:nexus /app /run/secrets

# Copy binary from builder
COPY --from=builder /app/target/release/nexus-server /usr/local/bin/nexus-server
RUN chmod +x /usr/local/bin/nexus-server

# Set working directory
WORKDIR /app

# Switch to non-root user
USER nexus

# Expose default port
EXPOSE 15474

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:15474/health || exit 1

# Default environment variables
ENV NEXUS_ADDR=0.0.0.0:15474
ENV NEXUS_DATA_DIR=/app/data
ENV RUST_LOG=info

# Run server
CMD ["nexus-server"]

