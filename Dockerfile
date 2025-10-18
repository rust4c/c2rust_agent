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

# System packages: build tools, VCS, SSL, SQLite, LLVM/Clang v18 (with CMake files), FUSE, etc.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    nano \
    ca-certificates \
    curl \
    wget \
    git \
    build-essential \
    pkg-config \
    cmake \
    python3 \
    python3-pip \
    llvm-18 \
    llvm-18-dev \
    clang-18 \
    libclang-18-dev \
    libssl-dev \
    zlib1g-dev \
    libsqlite3-dev \
    libglib2.0-dev \
    libfuse-dev \
    tzdata \
    openssh-server \
    openssh-client \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Make LLVM/Clang discoverable by CMake (for c2rust-ast-exporter)
ENV LLVM_VERSION=18 \
    CMAKE_PREFIX_PATH=/usr/lib/llvm-18/lib/cmake \
    LLVM_DIR=/usr/lib/llvm-18/lib/cmake/llvm \
    Clang_DIR=/usr/lib/llvm-18/lib/cmake/clang

# Sanity check: ensure LLVMConfig.cmake exists
RUN test -f /usr/lib/llvm-18/lib/cmake/llvm/LLVMConfig.cmake

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

# Install and configure FastGithub for GitHub acceleration
RUN set -eux; \
    mkdir -p /opt/fastgithub; \
    cd /opt/fastgithub; \
    # Try the recommended version first, fallback to alternative if needed
    wget -O fastgithub.tar.gz "https://gh-proxy.com/https://github.com/creazyboyone/FastGithub/releases/download/v2.1.5/fastgithub-linux-x64-ok.tar.gz" || \
    wget -O fastgithub.tar.gz "https://gh-proxy.com/https://github.com/creazyboyone/FastGithub/releases/download/v2.1.5/fastgithub-linux-x64.tar.gz"; \
    tar -xzf fastgithub.tar.gz --strip-components=1 || tar -xzf fastgithub.tar.gz; \
    rm fastgithub.tar.gz; \
    chmod +x fastgithub; \
    # Create directories for FastGithub
    mkdir -p /etc/fastgithub /var/log/fastgithub; \
    # Configure Git to work with FastGithub proxy
    git config --global http.sslverify false; \
    git config --global https.sslverify false

WORKDIR /opt/c2rust_agent

# Copy manifest files first for better build caching
RUN git clone https://github.com/rust4c/c2rust_agent.git .
RUN mv test-projects/translate_chibicc translate_chibicc
RUN mv test-projects/translate_littlefs_fuse translate_littlefs_fuse

# Install c2rust using versioned llvm-config for reliable detection
ENV LLVM_CONFIG_PATH=/usr/bin/llvm-config-18
RUN cargo install --git https://github.com/immunant/c2rust.git c2rust

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

# Expose SSH port and FastGithub proxy port
EXPOSE 22 38457

# Create startup script that runs FastGithub in background
RUN printf '%s\n' \
    '#!/bin/bash' \
    'set -e' \
    '' \
    '# Function to start FastGithub' \
    'start_fastgithub() {' \
    '    echo "Starting FastGithub..."' \
    '    cd /opt/fastgithub' \
    '    # Start FastGithub in background and redirect output to log' \
    '    ./fastgithub > /var/log/fastgithub/fastgithub.log 2>&1 &' \
    '    FASTGITHUB_PID=$!' \
    '    echo "FastGithub started with PID: $FASTGITHUB_PID"' \
    '    ' \
    '    # Wait for FastGithub to start' \
    '    for i in {1..10}; do' \
    '        if curl -s --connect-timeout 2 http://127.0.0.1:38457 > /dev/null 2>&1; then' \
    '            echo "FastGithub proxy is running on port 38457"' \
    '            # Configure git to use proxy once FastGithub is confirmed running' \
    '            git config --global http.proxy http://127.0.0.1:38457' \
    '            git config --global https.proxy http://127.0.0.1:38457' \
    '            export HTTP_PROXY=http://127.0.0.1:38457' \
    '            export HTTPS_PROXY=http://127.0.0.1:38457' \
    '            export http_proxy=http://127.0.0.1:38457' \
    '            export https_proxy=http://127.0.0.1:38457' \
    '            return 0' \
    '        fi' \
    '        echo "Waiting for FastGithub to start... ($i/10)"' \
    '        sleep 2' \
    '    done' \
    '    echo "Warning: FastGithub may not be running properly"' \
    '    return 1' \
    '}' \
    '' \
    '# Start FastGithub' \
    'start_fastgithub || echo "FastGithub failed to start, continuing without proxy..."' \
    '' \
    '# Execute the original command' \
    'exec "$@"' \
    > /usr/local/bin/start-with-fastgithub.sh && \
    chmod +x /usr/local/bin/start-with-fastgithub.sh

# Use the startup script as entrypoint, but keep bash as default command
ENTRYPOINT ["/usr/local/bin/start-with-fastgithub.sh"]
CMD ["bash"]
