# Dockerfile - Standalone Fortémi API (requires external PostgreSQL)
#
# Usage:
#   docker build -t fortemi:api .
#   docker run -d -p 3000:3000 -e DATABASE_URL=postgres://... fortemi:api
#
# For all-in-one deployment with embedded PostgreSQL, use Dockerfile.bundle instead.
#
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

LABEL org.opencontainers.image.title="fortemi"
LABEL org.opencontainers.image.description="AI-enhanced knowledge base with semantic search"
LABEL org.opencontainers.image.version="${VERSION}"
LABEL org.opencontainers.image.revision="${GIT_SHA}"
LABEL org.opencontainers.image.created="${BUILD_DATE}"
LABEL org.opencontainers.image.source="https://github.com/fortemi/fortemi"
LABEL org.opencontainers.image.vendor="fortemi"

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

# =============================================================================
# Version environment variables
# =============================================================================
ENV MATRIC_VERSION=${VERSION}
ENV MATRIC_GIT_SHA=${GIT_SHA}
ENV MATRIC_BUILD_DATE=${BUILD_DATE}

# =============================================================================
# API Server environment
# =============================================================================
# DATABASE_URL must be provided at runtime (required)
# ENV DATABASE_URL=postgres://matric:matric@db:5432/matric

ENV HOST=0.0.0.0
ENV PORT=3000
ENV RUST_LOG=info

# OAuth/Auth - Set ISSUER_URL to your external URL for OAuth discovery
# ENV ISSUER_URL=https://memory.example.com

# =============================================================================
# Rate Limiting
# =============================================================================
# Set to false to disable rate limiting (useful for development/testing)
# ENV RATE_LIMIT_ENABLED=true
# ENV RATE_LIMIT_REQUESTS=100
# ENV RATE_LIMIT_PERIOD_SECS=60

# =============================================================================
# Logging
# =============================================================================
# ENV LOG_FORMAT=json
# ENV LOG_FILE=/var/log/matric/api.log
# ENV LOG_ANSI=false

# =============================================================================
# Background Worker
# =============================================================================
# Set to false to disable background job processing
# ENV WORKER_ENABLED=true

# =============================================================================
# Backup Configuration
# =============================================================================
# ENV BACKUP_DEST=/var/backups/matric-memory
# ENV BACKUP_SCRIPT_PATH=/app/scripts/backup.sh

# =============================================================================
# Ollama (local LLM) - for embeddings and generation
# =============================================================================
# ENV OLLAMA_BASE=http://localhost:11434
# ENV OLLAMA_HOST=http://localhost:11434
# ENV OLLAMA_EMBED_MODEL=nomic-embed-text
# ENV OLLAMA_GEN_MODEL=llama3.2
# ENV OLLAMA_EMBED_DIM=768

# =============================================================================
# OpenAI (alternative to Ollama)
# =============================================================================
# ENV OPENAI_BASE_URL=https://api.openai.com/v1
# ENV OPENAI_API_KEY=sk-xxx
# ENV OPENAI_EMBED_MODEL=text-embedding-3-small
# ENV OPENAI_GEN_MODEL=gpt-4o-mini
# ENV OPENAI_EMBED_DIM=1536
# ENV OPENAI_TIMEOUT=30
# ENV OPENAI_SKIP_TLS_VERIFY=false

# Expose API port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Run the server
CMD ["/app/matric-api"]
