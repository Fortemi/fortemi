# matric-memory Builder Image
#
# Self-contained build environment for CI/CD. Contains all tools needed to
# build, test, lint, and package matric-memory.
#
# Usage:
#   docker build -f build/Dockerfile.builder -t matric-builder .
#   docker run -v $(pwd):/build -w /build matric-builder cargo test
#
# Registry:
#   git.integrolabs.net/roctinam/matric-memory/builder:latest

FROM rust:1.92-bookworm

# Build arguments
ARG RUST_VERSION=1.92.0
ARG SQLX_VERSION=0.8
ARG NODE_VERSION=22

# Labels
LABEL org.opencontainers.image.title="matric-builder"
LABEL org.opencontainers.image.description="Build environment for matric-memory CI/CD"
LABEL org.opencontainers.image.source="https://git.integrolabs.net/roctinam/matric-memory"
LABEL org.opencontainers.image.vendor="integrolabs"
LABEL matric.builder.rust-version="${RUST_VERSION}"
LABEL matric.builder.sqlx-version="${SQLX_VERSION}"

# Environment variables
ENV CARGO_TERM_COLOR=always
ENV CARGO_INCREMENTAL=0
ENV RUST_BACKTRACE=1
ENV RUSTFLAGS="-D warnings"

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Build essentials
    build-essential \
    pkg-config \
    libssl-dev \
    # Database
    libpq-dev \
    postgresql-client \
    # Docker CLI (for building images)
    docker.io \
    # Node.js for MCP server
    curl \
    gnupg \
    # Utilities
    git \
    jq \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js LTS
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Pin Rust version and add components
RUN rustup default ${RUST_VERSION} \
    && rustup component add rustfmt clippy rust-src

# Install cargo tools (use --locked to pin dependency versions)
RUN cargo install sqlx-cli --version "~${SQLX_VERSION}" --no-default-features --features postgres,rustls --locked \
    && cargo install cargo-watch --locked \
    && rm -rf /root/.cargo/registry /root/.cargo/git

# Create workspace directory
WORKDIR /build

# Copy entrypoint script
COPY build/scripts/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Default command - shell for interactive use
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["bash"]
