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
    net-tools \
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
    # Try multiple download sources with retries
    ( \
    wget --timeout=30 --tries=3 -O fastgithub.tar.gz "https://github.com/creazyboyone/FastGithub/releases/download/v2.1.5/fastgithub-linux-x64-ok.tar.gz" || \
    wget --timeout=30 --tries=3 -O fastgithub.tar.gz "https://github.com/creazyboyone/FastGithub/releases/download/v2.1.5/fastgithub-linux-x64.tar.gz" || \
    wget --timeout=30 --tries=3 -O fastgithub.tar.gz "https://ghproxy.com/https://github.com/creazyboyone/FastGithub/releases/download/v2.1.5/fastgithub-linux-x64-ok.tar.gz" || \
    wget --timeout=30 --tries=3 -O fastgithub.tar.gz "https://mirror.ghproxy.com/https://github.com/creazyboyone/FastGithub/releases/download/v2.1.5/fastgithub-linux-x64-ok.tar.gz" \
    ) && \
    # Extract the archive
    ( \
    tar -xzf fastgithub.tar.gz --strip-components=1 2>/dev/null || \
    tar -xzf fastgithub.tar.gz \
    ) && \
    rm -f fastgithub.tar.gz && \
    # Find and setup the FastGithub binary
    ( \
    chmod +x fastgithub 2>/dev/null || \
    find . -name "fastgithub" -executable -exec chmod +x {} \; || \
    find . -name "fastgithub" -exec chmod +x {} \; \
    ) && \
    # Move binary to current directory if needed
    if [ ! -f "./fastgithub" ]; then \
    find . -name "fastgithub" -executable -exec mv {} ./ \; 2>/dev/null || \
    find . -name "fastgithub" -exec mv {} ./ \; ; \
    fi && \
    # Move cacert directory if it exists
    find . -name "cacert" -type d -exec mv {} ./ \; 2>/dev/null || true && \
    # Clean up extracted directories
    find . -mindepth 1 -maxdepth 1 -type d -name "fastgithub*" -exec rm -rf {} \; 2>/dev/null || true && \
    echo "FastGithub installation completed" || \
    # Fallback: create dummy fastgithub if download failed
    ( \
    echo "Warning: FastGithub download failed, creating dummy binary"; \
    echo '#!/bin/bash' > fastgithub; \
    echo 'echo "FastGithub not available - using direct connection"' >> fastgithub; \
    echo 'sleep 3600' >> fastgithub; \
    chmod +x fastgithub \
    ); \
    # Create directories for FastGithub
    mkdir -p /etc/fastgithub /var/log/fastgithub; \
    # Configure Git to work with FastGithub proxy (will be activated when FastGithub starts)
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
    '#!/bin/sh' \
    'set -e' \
    '' \
    '# Check if we should skip FastGithub' \
    'if [ "$SKIP_FASTGITHUB" = "1" ] || [ "$SKIP_FASTGITHUB" = "true" ]; then' \
    '    echo "Skipping FastGithub startup (SKIP_FASTGITHUB is set)"' \
    '    exec "$@"' \
    '    exit 0' \
    'fi' \
    '' \
    '# Check if FastGithub is available' \
    'if [ ! -x "/opt/fastgithub/fastgithub" ]; then' \
    '    echo "FastGithub not available, using direct connection"' \
    '    exec "$@"' \
    '    exit 0' \
    'fi' \
    '' \
    '# Check if port is already in use' \
    'if netstat -tuln 2>/dev/null | grep -q ":38457 "; then' \
    '    echo "Port 38457 already in use, skipping FastGithub startup"' \
    '    exec "$@"' \
    '    exit 0' \
    'fi' \
    '' \
    'echo "Starting FastGithub..."' \
    'cd /opt/fastgithub' \
    '' \
    '# Start FastGithub in background' \
    'nohup ./fastgithub > /var/log/fastgithub/fastgithub.log 2>&1 &' \
    'FASTGITHUB_PID=$!' \
    'echo "FastGithub started with PID: $FASTGITHUB_PID"' \
    '' \
    '# Wait for FastGithub to start (simple loop)' \
    'i=1' \
    'while [ $i -le 15 ]; do' \
    '    if curl -s --connect-timeout 3 --max-time 5 http://127.0.0.1:38457 >/dev/null 2>&1; then' \
    '        echo "FastGithub proxy is running on port 38457"' \
    '        # Configure git to use proxy' \
    '        git config --global http.proxy http://127.0.0.1:38457' \
    '        git config --global https.proxy http://127.0.0.1:38457' \
    '        export HTTP_PROXY=http://127.0.0.1:38457' \
    '        export HTTPS_PROXY=http://127.0.0.1:38457' \
    '        export http_proxy=http://127.0.0.1:38457' \
    '        export https_proxy=http://127.0.0.1:38457' \
    '        break' \
    '    fi' \
    '    echo "Waiting for FastGithub to start... ($i/15)"' \
    '    sleep 2' \
    '    i=$((i+1))' \
    'done' \
    '' \
    'if [ $i -gt 15 ]; then' \
    '    echo "Warning: FastGithub may not be running properly"' \
    'fi' \
    '' \
    '# Execute the original command' \
    'exec "$@"' \
    > /usr/local/bin/start-with-fastgithub.sh && \
    chmod +x /usr/local/bin/start-with-fastgithub.sh

# Use the startup script as entrypoint, but keep bash as default command
ENTRYPOINT ["/usr/local/bin/start-with-fastgithub.sh"]
CMD ["bash"]
