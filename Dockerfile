# syntax=docker/dockerfile:1.3

# Stage 1: Install cargo-chef and build dependencies
FROM --platform=$BUILDPLATFORM rustlang/rust:nightly-alpine AS chef
WORKDIR /app
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static
RUN cargo install --locked cargo-chef
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

# Stage 2: Prepare recipe.json
FROM chef AS planner
COPY Cargo.toml Cargo.lock* ./
COPY src src
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Build dependencies (cached layer)
FROM chef AS builder
ARG TARGETPLATFORM
COPY --from=planner /app/recipe.json recipe.json

# Determine target based on platform
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") echo "x86_64-unknown-linux-musl" > /tmp/target ;; \
    "linux/arm64") echo "aarch64-unknown-linux-musl" > /tmp/target ;; \
    *) echo "x86_64-unknown-linux-musl" > /tmp/target ;; \
    esac

RUN export CARGO_TARGET=$(cat /tmp/target) && \
    cargo chef cook --recipe-path recipe.json --release --target $CARGO_TARGET

# Stage 4: Build the application
COPY . .
RUN export CARGO_TARGET=$(cat /tmp/target) && \
    cargo build --release --target $CARGO_TARGET && \
    cp target/$CARGO_TARGET/release/cyberfly-rust-node /app/cyberfly-rust-node && \
    strip /app/cyberfly-rust-node

# Stage 5: Runtime image
FROM alpine:latest AS runtime

# Install runtime dependencies
RUN apk add --no-cache ca-certificates && \
    adduser -D -s /bin/sh cyberfly

WORKDIR /app

# Create data directory and set ownership
RUN mkdir -p /app/data/iroh && \
    chown -R cyberfly:cyberfly /app

# Copy the binary
COPY --from=builder --chown=cyberfly:cyberfly /app/cyberfly-rust-node /app/cyberfly-rust-node
COPY --chown=cyberfly:cyberfly schema.graphql /app/schema.graphql

# Switch to non-root user
USER cyberfly

# Expose ports
EXPOSE 31001 31002 31003 31006

# Run the application
CMD ["/app/cyberfly-rust-node"]