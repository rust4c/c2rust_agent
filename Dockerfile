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
    systemd-resolved \
    dnsutils \
    curl \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Configure Cloudflare DNS with DoH/DoT support
RUN mkdir -p /etc/systemd/resolved.conf.d && \
    printf '%s\n' \
    '[Resolve]' \
    'DNS=1.1.1.1#cloudflare-dns.com 1.0.0.1#cloudflare-dns.com 2606:4700:4700::1111#cloudflare-dns.com 2606:4700:4700::1001#cloudflare-dns.com' \
    'DNSOverTLS=yes' \
    'DNSSEC=yes' \
    'Cache=yes' \
    'DNSStubListener=yes' \
    > /etc/systemd/resolved.conf.d/cloudflare.conf && \
    # Backup original resolv.conf if accessible and configure Cloudflare DNS as fallback
    if [ -w /etc/resolv.conf ]; then \
    cp /etc/resolv.conf /etc/resolv.conf.bak || true; \
    fi && \
    # Write fallback resolv to /usr/local/etc/resolv.conf (read-only /etc/ during buildkit)
    mkdir -p /usr/local/etc && \
    printf '%s\n' \
    '# Cloudflare DNS servers' \
    'nameserver 1.1.1.1' \
    'nameserver 1.0.0.1' \
    'nameserver 2606:4700:4700::1111' \
    'nameserver 2606:4700:4700::1001' \
    'options timeout:2' \
    'options attempts:3' \
    'options rotate' \
    > /usr/local/etc/resolv.conf && \
    # Create a script to start systemd-resolved if needed
    printf '%s\n' \
    '#!/bin/bash' \
    'if ! systemctl is-active --quiet systemd-resolved; then' \
    '    systemctl start systemd-resolved' \
    'fi' \
    > /usr/local/bin/start-resolved && \
    chmod +x /usr/local/bin/start-resolved && \
    # Install cloudflared for DoH support
    curl -L --output cloudflared.deb https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb && \
    dpkg -i cloudflared.deb && \
    rm cloudflared.deb && \
    # Configure cloudflared as DNS proxy
    mkdir -p /etc/cloudflared && \
    printf '%s\n' \
    'proxy-dns: true' \
    'proxy-dns-upstream:' \
    '  - https://1.1.1.1/dns-query' \
    '  - https://1.0.0.1/dns-query' \
    'proxy-dns-address: 127.0.0.1' \
    'proxy-dns-port: 5053' \
    > /etc/cloudflared/config.yml && \
    # Create DNS verification script
    printf '%s\n' \
    '#!/bin/bash' \
    'echo "Testing DNS resolution..."' \
    'echo "Standard DNS (1.1.1.1):"' \
    'nslookup cloudflare.com 1.1.1.1 || echo "Standard DNS failed"' \
    'echo ""' \
    'echo "DoH via cloudflared:"' \
    'if pgrep cloudflared > /dev/null; then' \
    '    nslookup cloudflare.com 127.0.0.1 -port=5053 || echo "DoH DNS failed"' \
    'else' \
    '    echo "cloudflared not running"' \
    'fi' \
    'echo ""' \
    'echo "Current DNS config (build-time fallback shown if set):"' \
    'if [ -f /usr/local/etc/resolv.conf ]; then' \
    '    cat /usr/local/etc/resolv.conf' \
    'else' \
    '    cat /etc/resolv.conf || true' \
    'fi' \
    > /usr/local/bin/test-dns && \
    chmod +x /usr/local/bin/test-dns

# Configure Git for better HTTPS handling and retry logic:cite[5]
RUN git config --global http.postBuffer 524288000 && \
    git config --global http.lowSpeedLimit 0 && \
    git config --global http.lowSpeedTime 999999

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

# Configure Cargo to use TUNA mirror for crates.io:cite[10]
RUN mkdir -p /root/.cargo && \
    printf '%s\n' \
    "[source.crates-io]" \
    "replace-with = 'mirror'" \
    "" \
    "[source.mirror]" \
    "registry = \"sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/\"" \
    "" \
    "[net]" \
    "retry-delay = 30" \
    "git-fetch-with-cli = true" \
    "" \
    "[registries.mirror]" \
    "index = \"sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/\"" \
    > /root/.cargo/config.toml

WORKDIR /opt/c2rust_agent

# Copy manifest files first for better build caching
# Use shallow clone and retry logic for git operations:cite[5]
RUN git clone https://github.com/rust4c/c2rust_agent.git . --depth 1 && \
    mv test-projects/translate_chibicc translate_chibicc && \
    mv test-projects/translate_littlefs_fuse translate_littlefs_fuse

# Enhanced cargo install with retry mechanism for c2rust
RUN set -eux; \
    MAX_RETRIES=5; \
    COUNT=0; \
    until cargo install --git https://github.com/immunant/c2rust.git c2rust; do \
    COUNT=$$((COUNT+1)); \
    if [ $$COUNT -eq $$MAX_RETRIES ]; then \
    echo "Failed to install c2rust after $$MAX_RETRIES attempts"; \
    exit 1; \
    fi; \
    echo "Attempt $$COUNT failed. Retrying in 30 seconds..."; \
    sleep 30; \
    done

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

# Create startup script that initializes DNS and starts bash
RUN printf '%s\n' \
    '#!/bin/bash' \
    'set -e' \
    '# Initialize DNS services' \
    '/usr/local/bin/start-resolved || true' \
    '# Start cloudflared DoH proxy in background' \
    'if [ "$ENABLE_DOH" = "yes" ]; then' \
    '    cloudflared proxy-dns --config /etc/cloudflared/config.yml &' \
    '    sleep 2' \
    '    # Update resolv.conf to use DoH proxy' \
    '    printf "%s\\n" "nameserver 127.0.0.1" "nameserver 1.1.1.1" > /etc/resolv.conf' \
    'fi' \
    '# Start SSH daemon if requested' \
    'if [ "$START_SSH" = "yes" ]; then' \
    '    service ssh start' \
    'fi' \
    '# Execute the original command or start bash' \
    'exec "$@"' \
    > /usr/local/bin/docker-entrypoint.sh && \
    chmod +x /usr/local/bin/docker-entrypoint.sh

# Default shell with DNS initialization
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["bash"]
