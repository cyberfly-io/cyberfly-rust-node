# syntax=docker/dockerfile:1.3
# Build stage - Use Rust Alpine with musl
FROM rustlang/rust:nightly-alpine AS builder

WORKDIR /app

# Install build dependencies including OpenSSL for musl
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static

# Set up Rust compilation cache (speeds up incremental builds)
ENV CARGO_INCREMENTAL=1
ENV CARGO_HOME=/usr/local/cargo

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock* ./

# Create a dummy main.rs to build dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Build dependencies with cache mount
# When Cargo.toml changes, this layer rebuilds but uses cached registry downloads
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release

# Copy source code
COPY src ./src
COPY schema.graphql ./

# Build the actual application with cache mounts
# Touch source files to force rebuild of app code only, not dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    find src -type f -name "*.rs" -exec touch {} + && \
    cargo build --release && \
    cp /app/target/release/cyberfly-rust-node /app/cyberfly-rust-node-final && \
    strip /app/cyberfly-rust-node-final

# Runtime stage - Minimal Alpine
FROM alpine:latest

# Install only essential runtime dependencies
RUN apk add --no-cache ca-certificates && \
    adduser -D -s /bin/sh cyberfly

WORKDIR /app

# Create data directory and set ownership before copying files
RUN mkdir -p /app/data/iroh && \
    chown -R cyberfly:cyberfly /app

# Copy the statically linked binary and schema
COPY --from=builder --chown=cyberfly:cyberfly /app/cyberfly-rust-node-final /app/cyberfly-rust-node
COPY --from=builder --chown=cyberfly:cyberfly /app/schema.graphql /app/schema.graphql

# Switch to non-root user
USER cyberfly

# Expose port
EXPOSE 31001 31002 31003 31006

# Run the application
CMD ["/app/cyberfly-rust-node"]