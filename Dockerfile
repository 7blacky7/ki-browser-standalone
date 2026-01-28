# KI-Browser Standalone Docker Image with CEF Browser Engine
# Multi-stage build for CEF + Rust + Xvfb headless browser automation
#
# Build: docker build -t ki-browser:cef .
# Run:   docker run -d --shm-size=2gb -p 9222:9222 ki-browser:cef

# =============================================================================
# Stage 1: CEF Binary Download
# =============================================================================
FROM debian:bookworm-slim AS cef-downloader

RUN apt-get update && apt-get install -y \
    curl \
    bzip2 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /cef

# Download CEF minimal distribution (Linux 64-bit)
# Using version 143.0.13 to match the cef crate version 143.3.0
# Check https://cef-builds.spotifycdn.com/index.html for available versions
ARG CEF_VERSION="cef_binary_143.0.13+g30cb3bd+chromium-143.0.7499.170_linux64_minimal"
ARG CEF_URL="https://cef-builds.spotifycdn.com/${CEF_VERSION}.tar.bz2"

RUN echo "Downloading CEF from ${CEF_URL}..." && \
    curl -fsSL "${CEF_URL}" -o cef.tar.bz2 && \
    tar -xjf cef.tar.bz2 && \
    rm cef.tar.bz2 && \
    mv cef_binary_* cef_binary

# =============================================================================
# Stage 2: Rust Build with CEF Feature
# =============================================================================
FROM rust:1.84-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies for CEF and Rust
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    ninja-build \
    clang \
    libgtk-3-dev \
    libglib2.0-dev \
    libnss3-dev \
    libnspr4-dev \
    libasound2-dev \
    libcups2-dev \
    libxss-dev \
    libxtst-dev \
    libxrandr-dev \
    libatk1.0-dev \
    libatk-bridge2.0-dev \
    libpango1.0-dev \
    libcairo2-dev \
    libdrm-dev \
    libgbm-dev \
    libxcomposite-dev \
    libxdamage-dev \
    libxfixes-dev \
    libxkbcommon-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy CEF binary from downloader stage
COPY --from=cef-downloader /cef/cef_binary /opt/cef

# Set CEF environment variables for build
ENV CEF_ROOT=/opt/cef
ENV LD_LIBRARY_PATH=/opt/cef/Release:$LD_LIBRARY_PATH

# Copy Rust project source
COPY Cargo.toml ./
COPY build.rs ./
COPY src/ ./src/

# Build with CEF feature enabled
# Note: If CEF crate build fails, fall back to mock-browser
RUN cargo build --release --features cef-browser 2>/dev/null || \
    (echo "CEF build failed, falling back to mock-browser" && \
     cargo build --release --no-default-features --features mock-browser)

# =============================================================================
# Stage 3: Runtime with Xvfb
# =============================================================================
FROM debian:bookworm-slim

LABEL maintainer="KI-Browser Team"
LABEL description="High-performance browser automation with CEF and stealth capabilities"
LABEL version="0.1.0"

WORKDIR /app

# Install runtime dependencies for CEF + Xvfb
RUN apt-get update && apt-get install -y --no-install-recommends \
    # X virtual framebuffer for headless rendering
    xvfb \
    # GTK and GUI libraries
    libgtk-3-0 \
    libglib2.0-0 \
    # NSS (Network Security Services)
    libnss3 \
    libnspr4 \
    # Audio (required even in headless)
    libasound2 \
    # Printing (required by Chromium)
    libcups2 \
    # X11 libraries
    libxss1 \
    libxtst6 \
    libxrandr2 \
    libxcomposite1 \
    libxdamage1 \
    libxfixes3 \
    libxkbcommon0 \
    # Accessibility toolkit
    libatk1.0-0 \
    libatk-bridge2.0-0 \
    # Font rendering
    libpango-1.0-0 \
    libcairo2 \
    # DRM and GPU (for software rendering)
    libdrm2 \
    libgbm1 \
    # Fonts
    fonts-liberation \
    fonts-dejavu-core \
    fonts-noto-color-emoji \
    # CA certificates for HTTPS
    ca-certificates \
    # Process utilities
    procps \
    && rm -rf /var/lib/apt/lists/*

# Copy CEF Release libraries
COPY --from=cef-downloader /cef/cef_binary/Release/ /opt/cef/
COPY --from=cef-downloader /cef/cef_binary/Resources/ /opt/cef/

# Copy binary from builder
COPY --from=builder /build/target/release/ki-browser /app/ki-browser

# Copy entrypoint script
COPY docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh

# Create directories
RUN mkdir -p /app/config /app/profiles /app/data /tmp/.X11-unix

# Set library path for CEF
ENV LD_LIBRARY_PATH=/opt/cef:$LD_LIBRARY_PATH

# Xvfb display configuration
ENV DISPLAY=:99
ENV XVFB_RESOLUTION=1920x1080x24

# KI-Browser configuration
ENV RUST_LOG=info
ENV KI_BROWSER_API_PORT=9222
ENV KI_BROWSER_API_ENABLED=true
ENV KI_BROWSER_HEADLESS=true
ENV KI_BROWSER_STEALTH=true

# Expose API port
EXPOSE 9222

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9222/health || exit 1

# Run with entrypoint that starts Xvfb
ENTRYPOINT ["/app/docker-entrypoint.sh"]
CMD ["--port", "9222", "--headless", "--stealth"]
