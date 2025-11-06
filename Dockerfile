# Dockerfile for pre-built binaries
# Build binaries with: cross build --release --target <target>
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache ca-certificates libgcc && \
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

# Run the application
CMD ["/app/cyberfly-rust-node"]