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

FROM node:22-bookworm-slim@sha256:6c74791e557ce11fc957704f6d4fe134a7bc8d6f5ca4403205b2966bd488f6b3 AS node-runtime

FROM rust:1.92-bookworm@sha256:e90e846de4124376164ddfbaab4b0774c7bdeef5e738866295e5a90a34a307a2

# Build arguments
ARG RUST_VERSION=1.92.0
ARG SQLX_VERSION=0.8
ARG DOCKER_CLI_VERSION=5:29.6.2-1~debian.12~bookworm
ARG DOCKER_BUILDX_VERSION=0.35.0-1~debian.12~bookworm

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
    libatomic1 \
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
    && gpg --batch --show-keys --with-colons /etc/apt/keyrings/docker.asc \
        | grep -q 'fpr:::::::::9DC858229FC7DD38854AE2D88D81803C0EBFCD88:' \
    && chmod a+r /etc/apt/keyrings/docker.asc \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian bookworm stable" \
        > /etc/apt/sources.list.d/docker.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        docker-ce-cli="${DOCKER_CLI_VERSION}" \
        docker-buildx-plugin="${DOCKER_BUILDX_VERSION}" \
    && install -d /usr/share/fortemi \
    && dpkg-query -W -f='${Package}\t${Version}\n' \
        docker-ce-cli docker-buildx-plugin \
        > /usr/share/fortemi/builder-third-party-packages.tsv \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js LTS from the reviewed official image without adding another
# external apt repository to the trusted builder.
COPY --from=node-runtime /usr/local/bin/node /usr/local/bin/node
COPY --from=node-runtime /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx

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
