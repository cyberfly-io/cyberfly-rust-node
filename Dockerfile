# syntax=docker/dockerfile:1.3

# Stage 1: Install cargo-chef and build dependencies
FROM --platform=$BUILDPLATFORM rust:alpine AS chef
WORKDIR /app
ENV PKG_CONFIG_SYSROOT_DIR=/
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static zig
RUN cargo install --locked cargo-zigbuild cargo-chef
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

# Stage 2: Prepare recipe.json
FROM chef AS planner
COPY Cargo.toml Cargo.lock* ./
COPY src src
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Build dependencies (cached layer)
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json --release --zigbuild \
    --target x86_64-unknown-linux-musl --target aarch64-unknown-linux-musl

# Stage 4: Build the application for both architectures
COPY . .
RUN cargo zigbuild --release \
    --target x86_64-unknown-linux-musl --target aarch64-unknown-linux-musl && \
    mkdir -p /app/linux/amd64 /app/linux/arm64 && \
    cp target/x86_64-unknown-linux-musl/release/cyberfly-rust-node /app/linux/amd64/cyberfly-rust-node && \
    cp target/aarch64-unknown-linux-musl/release/cyberfly-rust-node /app/linux/arm64/cyberfly-rust-node && \
    strip /app/linux/amd64/cyberfly-rust-node && \
    strip /app/linux/arm64/cyberfly-rust-node

# Stage 5: Runtime image
FROM alpine:latest AS runtime

# Install runtime dependencies
RUN apk add --no-cache ca-certificates && \
    adduser -D -s /bin/sh cyberfly

WORKDIR /app

# Create data directory and set ownership
RUN mkdir -p /app/data/iroh && \
    chown -R cyberfly:cyberfly /app

# Copy the correct binary based on target platform
ARG TARGETPLATFORM
COPY --from=builder --chown=cyberfly:cyberfly /app/${TARGETPLATFORM}/cyberfly-rust-node /app/cyberfly-rust-node
COPY --chown=cyberfly:cyberfly schema.graphql /app/schema.graphql

# Switch to non-root user
USER cyberfly

# Expose ports
EXPOSE 31001 31002 31003 31006

# Run the application
CMD ["/app/cyberfly-rust-node"]