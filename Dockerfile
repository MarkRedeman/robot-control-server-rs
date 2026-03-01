# syntax=docker/dockerfile:1

# ============================================================
# Stage 1: Chef (Prepare dependencies)
# ============================================================
FROM lukemathwalker/cargo-chef:latest-rust-1.93.0-bookworm AS chef
WORKDIR /app

# ============================================================
# Stage 2: Recipe (Compute dependency instructions)
# ============================================================
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ============================================================
# Stage 3: Builder
# ============================================================
FROM chef AS builder

# Install system dependencies for nokhwa (V4L2), serialport, and image processing
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libv4l-dev \
    libjpeg-dev \
    libusb-1.0-0-dev \
    libssl-dev \
    libudev-dev \
    libclang-dev \
    llvm-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

ENV CARGO_PROFILE_RELEASE_LTO=true \
    CARGO_PROFILE_RELEASE_PANIC=abort \
    CARGO_PROFILE_RELEASE_STRIP=symbols \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 

# Build application
COPY . .
# We just copy the entire context (since docker-compose context is `server/`)
# Build release binary
RUN cargo build --release --bin server

# ============================================================
# Stage 4: Runtime
# ============================================================
FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.source="https://github.com/MarkRedeman/robot-control-server-rs" \
      org.opencontainers.image.description="A Rust server for real-time robot arm control over WebSocket and REST, with servo calibration, camera streaming, and a CLI for diagnostics." \
      org.opencontainers.image.revision="${GIT_SHA}"

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    libv4l-0 \
    libjpeg62-turbo \
    libusb-1.0-0 \
    libssl3 \
    udev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd --gid 1000 appuser \
    && useradd --uid 1000 --gid 1000 --create-home appuser

WORKDIR /app

# Copy binary from builder
COPY --from=builder --chown=appuser:appuser /app/target/release/server /app/

USER appuser

ENV RUST_LOG=info

EXPOSE 8000

# Healthcheck
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8000/health || exit 1

CMD ["/app/server"]
