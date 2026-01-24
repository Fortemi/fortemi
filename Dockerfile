# Build stage
FROM rust:slim-bookworm AS builder

# Build arguments for version stamping
ARG VERSION=dev
ARG GIT_SHA=unknown
ARG BUILD_DATE=unknown

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary with version info
ENV MATRIC_VERSION=${VERSION}
ENV MATRIC_GIT_SHA=${GIT_SHA}
ENV MATRIC_BUILD_DATE=${BUILD_DATE}

RUN cargo build --release --package matric-api && \
    cp target/release/matric-api /app/matric-api

# Runtime stage
FROM debian:bookworm-slim AS runtime

# Version labels
ARG VERSION=dev
ARG GIT_SHA=unknown
ARG BUILD_DATE=unknown

LABEL org.opencontainers.image.title="matric-memory"
LABEL org.opencontainers.image.description="AI-enhanced knowledge base with semantic search"
LABEL org.opencontainers.image.version="${VERSION}"
LABEL org.opencontainers.image.revision="${GIT_SHA}"
LABEL org.opencontainers.image.created="${BUILD_DATE}"
LABEL org.opencontainers.image.source="https://git.integrolabs.net/roctinam/matric-memory"
LABEL org.opencontainers.image.vendor="integrolabs"

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --user-group matric

# Copy binary from builder
COPY --from=builder /app/matric-api /app/matric-api

# Copy migrations
COPY migrations /app/migrations

# Set ownership
RUN chown -R matric:matric /app

USER matric

# Version environment variables (available at runtime)
ENV MATRIC_VERSION=${VERSION}
ENV MATRIC_GIT_SHA=${GIT_SHA}
ENV MATRIC_BUILD_DATE=${BUILD_DATE}

# Expose API port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Run the server
ENV RUST_LOG=info
CMD ["/app/matric-api"]
