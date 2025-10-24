# Build stage - Use Rust Alpine with musl
FROM rustlang/rust:nightly-alpine as builder

WORKDIR /app

# Install musl build tools
RUN apk add --no-cache musl-dev

# Copy dependency files first for better caching
COPY Cargo.toml ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy source code
COPY src ./src
COPY schema.graphql ./

# Build the actual application with musl (statically linked)
RUN cargo build --release

# Runtime stage - Minimal Alpine
FROM alpine:latest

# Install only essential runtime dependencies
RUN apk add --no-cache ca-certificates

# Create app user
RUN adduser -D -s /bin/sh cyberfly

# Create app directory
WORKDIR /app

# Copy the statically linked binary
COPY --from=builder /app/target/release/cyberfly-rust-node /app/cyberfly-rust-node
COPY --from=builder /app/schema.graphql /app/schema.graphql

# Set ownership
RUN chown -R cyberfly:cyberfly /app

# Switch to non-root user
USER cyberfly

# Expose port
EXPOSE 8080

# Run the application
CMD ["/app/cyberfly-rust-node"]