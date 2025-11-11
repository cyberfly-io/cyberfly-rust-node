# Dockerfile for musl-based static binaries
# Build binaries with: cargo build --release --target x86_64-unknown-linux-musl
# Using Alpine for minimal image size with musl libc
FROM alpine:3.19

# Install only CA certificates (static binary needs nothing else)
RUN apk add --no-cache ca-certificates && \
    adduser -D -s /bin/sh cyberfly

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