# Fortémi Builder Image
#
# Self-contained build environment for CI/CD. Contains all tools needed to
# build, test, lint, and package Fortémi.
#
# Usage:
#   docker build -f build/Dockerfile.builder -t fortemi-builder .
#   docker run -v $(pwd):/build -w /build fortemi-builder cargo test
#
# Registry:
#   ghcr.io/fortemi/fortemi-builder:latest

FROM rust:1.92-bookworm

# Build arguments
ARG RUST_VERSION=1.92.0
ARG SQLX_VERSION=0.8
ARG NODE_VERSION=22

# Labels
LABEL org.opencontainers.image.title="fortemi-builder"
LABEL org.opencontainers.image.description="Build environment for Fortémi CI/CD"
LABEL org.opencontainers.image.source="https://github.com/fortemi/fortemi"
LABEL org.opencontainers.image.vendor="fortemi"
LABEL matric.builder.rust-version="${RUST_VERSION}"
LABEL matric.builder.sqlx-version="${SQLX_VERSION}"

# Environment variables
ENV CARGO_TERM_COLOR=always
ENV CARGO_INCREMENTAL=0
ENV RUST_BACKTRACE=1
ENV RUSTFLAGS="-D warnings"

# Install system dependencies (excluding Docker CLI — installed separately below
# from Docker's official apt repo so we get a current client. Debian bookworm's
# docker.io ships CLI 20.10 / API 1.41 which is too old to talk to host daemons
# running Docker 25.0+ / API 1.44+. See issue #632.)
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Build essentials
    build-essential \
    pkg-config \
    libssl-dev \
    # Database
    libpq-dev \
    postgresql-client \
    # Media processing (required by audio/video pipeline)
    ffmpeg \
    # Node.js for MCP server
    curl \
    gnupg \
    # Utilities
    git \
    jq \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install current Docker CLI from Docker's official apt repo. We only need
# the CLI inside this builder — the daemon lives on the runner host and is
# reached via /var/run/docker.sock mount. Pinning to the upstream package
# guarantees the API version tracks the host daemon.
RUN install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg \
        -o /etc/apt/keyrings/docker.asc \
    && chmod a+r /etc/apt/keyrings/docker.asc \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian bookworm stable" \
        > /etc/apt/sources.list.d/docker.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends docker-ce-cli docker-buildx-plugin \
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
    && cargo install cargo-llvm-cov --locked \
    && rm -rf /root/.cargo/registry /root/.cargo/git

# Create workspace directory
WORKDIR /build

# Copy entrypoint script
COPY build/scripts/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Default command - shell for interactive use
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["bash"]
