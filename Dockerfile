# Dockerfile for pre-built binaries
# Build binaries with: cargo build --release --target <target>
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Create data directory
RUN mkdir -p /app/data/iroh

# Copy pre-built binary (path set by build context)
ARG TARGETARCH
COPY cyberfly-rust-node-${TARGETARCH} /app/cyberfly-rust-node
COPY schema.graphql /app/schema.graphql

# Ensure binary is executable
RUN chmod +x /app/cyberfly-rust-node

# Expose ports
EXPOSE 31001 31002 31003 31006

# Run the application
CMD ["/app/cyberfly-rust-node"]