#
# Competition-ready Docker image for c2rust_agent
# - Base: ubuntu:24.04 (matches competition host expectations)
# - Installs Rust toolchain (rustup) and common native build deps
# - Includes libfuse-dev to build translated littlefs_fuse
# - Builds the workspace (excluding GUI crate) and exposes CLI binaries on PATH
# - Leaves a developer-friendly environment (cargo, gcc, clang, pkg-config, cmake)
#

FROM ubuntu:24.04

LABEL org.opencontainers.image.title="c2rust_agent"
LABEL org.opencontainers.image.description="C to Rust translation toolchain image for competition evaluation"
LABEL org.opencontainers.image.source="https://example.invalid/repo"

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/root/.rustup \
    CARGO_HOME=/root/.cargo \
    PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    CARGO_NET_GIT_FETCH_WITH_CLI=true \
    RUST_LOG=info \
    # TUNA mirror for rustup
    RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup \
    RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup

# Switch APT sources to USTC mirror (supports DEB822 and legacy formats)
RUN set -eux; \
    if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then \
    sed -i 's@//ports.ubuntu.com@//mirrors.ustc.edu.cn@g' /etc/apt/sources.list.d/ubuntu.sources || true; \
    sed -i 's@//archive.ubuntu.com@//mirrors.ustc.edu.cn@g' /etc/apt/sources.list.d/ubuntu.sources || true; \
    sed -i 's@//security.ubuntu.com@//mirrors.ustc.edu.cn@g' /etc/apt/sources.list.d/ubuntu.sources || true; \
    fi; \
    if [ -f /etc/apt/sources.list ]; then \
    sed -i 's@//archive.ubuntu.com@//mirrors.ustc.edu.cn@g' /etc/apt/sources.list || true; \
    sed -i 's@//security.ubuntu.com@//mirrors.ustc.edu.cn@g' /etc/apt/sources.list || true; \
    sed -i 's@//ports.ubuntu.com@//mirrors.ustc.edu.cn@g' /etc/apt/sources.list || true; \
    fi

# System packages: build tools, VCS, SSL, SQLite, clang/LLVM, FUSE, etc.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    build-essential \
    pkg-config \
    cmake \
    glib-2.0-dev \
    python3 \
    python3-pip \
    clang \
    llvm \
    libclang-dev \
    libssl-dev \
    zlib1g-dev \
    libsqlite3-dev \
    libglib2.0-dev \
    libfuse-dev \
    tzdata \
    openssh-server \
    openssh-client \
    && rm -rf /var/lib/apt/lists/*

# Install Rust via rustup (stable) and useful components
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable && \
    rustup component add rustfmt clippy

# Configure Cargo to use TUNA mirror for crates.io
RUN mkdir -p /root/.cargo && \
    printf '%s\n' \
    "[source.crates-io]" \
    "replace-with = 'mirror'" \
    "" \
    "[source.mirror]" \
    "registry = \"sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/\"" \
    "" \
    "[registries.mirror]" \
    "index = \"sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/\"" \
    > /root/.cargo/config.toml

WORKDIR /opt/c2rust_agent

# Copy manifest files first for better build caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY config ./config
COPY .git ./.git
COPY README.md README-CN.md ./

# Build only the command-line tool as intended (avoids GUI deps)
RUN cargo build --release --locked -p commandline_tool

# Install compiledb (from PyPI) for generating compile_commands.json
RUN pip3 install --no-cache-dir --break-system-packages compiledb && \
    compiledb --version || true

# Install available CLI binaries to PATH (ignore if some are not present)
RUN set -eux; \
    bins="c2rust_agent commandline_tool project_remanager env_checker file_scanner main_processor single_processor"; \
    for b in $bins; do \
    if [ -f target/release/"$b" ]; then \
    install -Dm755 target/release/"$b" /usr/local/bin/"$b"; \
    fi; \
    done

# A neutral working directory for the checker to use as codegen_workdir
WORKDIR /workspace

# SSH server setup: create user and configure sshd
RUN set -eux; \
    useradd -m -s /bin/bash agent; \
    echo 'agent:agent' | chpasswd; \
    mkdir -p /home/agent/.ssh; \
    chmod 700 /home/agent/.ssh; \
    chown -R agent:agent /home/agent/.ssh; \
    sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication yes/' /etc/ssh/sshd_config; \
    sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin no/' /etc/ssh/sshd_config; \
    sed -i 's/^#\?PubkeyAuthentication .*/PubkeyAuthentication yes/' /etc/ssh/sshd_config; \
    printf '\nAllowUsers agent\nUseDNS no\nClientAliveInterval 60\nClientAliveCountMax 3\n' >> /etc/ssh/sshd_config

# Expose SSH port
EXPOSE 22

# Default shell; no entrypoint to give the checker full control
CMD ["bash"]
