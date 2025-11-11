# Dockerfile for pre-built binaries
# Build binaries with: cargo build --release --target <target>
# Using Debian Bullseye (GLIBC 2.31) for better compatibility
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/* && \
    useradd -m -s /bin/bash cyberfly

WORKDIR /app

# Create data directory and set ownership
RUN mkdir -p /app/data/iroh && \
    chown -R cyberfly:cyberfly /app

# Copy pre-built binary (path set by build context)
ARG TARGETARCH
COPY --chown=cyberfly:cyberfly cyberfly-rust-node-${TARGETARCH} /app/cyberfly-rust-node
COPY --chown=cyberfly:cyberfly schema.graphql /app/schema.graphql

# Ensure binary is executable
RUN chmod +x /app/cyberfly-rust-node

# Switch to non-root user
USER cyberfly

# Expose ports
EXPOSE 31001 31002 31003 31006

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/app/cyberfly-rust-node", "--version"] || exit 1

# Run the application
CMD ["/app/cyberfly-rust-node"]