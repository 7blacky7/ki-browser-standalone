# KI-Browser Standalone Docker Image
# Multi-stage build for minimal image size

FROM rust:1.84-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source code (exclude Cargo.lock to let cargo regenerate)
COPY Cargo.toml ./
COPY src/ ./src/

# Build release binary
RUN cargo build --release --no-default-features --features mock-browser

# Runtime stage - minimal image
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/ki-browser /app/ki-browser

# Create config directory
RUN mkdir -p /app/config /app/profiles

# Expose API port
EXPOSE 9222

# Set environment variables
ENV RUST_LOG=info
ENV KI_BROWSER_API_PORT=9222
ENV KI_BROWSER_API_ENABLED=true

# Run the browser
ENTRYPOINT ["/app/ki-browser"]
CMD ["--port", "9222"]
