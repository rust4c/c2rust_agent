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
		RUST_LOG=info

# System packages: build tools, VCS, SSL, SQLite, clang/LLVM, FUSE, etc.
RUN apt-get update && \
		apt-get install -y --no-install-recommends \
			ca-certificates \
			curl \
			git \
			build-essential \
			pkg-config \
			cmake \
			python3 \
			clang \
			llvm \
			libclang-dev \
			libssl-dev \
			zlib1g-dev \
			libsqlite3-dev \
			libfuse-dev \
			tzdata \
		&& rm -rf /var/lib/apt/lists/*

# Install Rust via rustup (stable) and useful components
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable && \
		rustup component add rustfmt clippy

WORKDIR /opt/c2rust_agent

# Copy manifest files first for better build caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY config ./config
COPY README.md README-CN.md ./

# Build release binaries, excluding GUI crate that may require extra system libs
# If your workspace does not include `ui_main`, the exclude is harmless.
RUN cargo build --release --locked --workspace --exclude ui_main

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

# Default shell; no entrypoint to give the checker full control
CMD ["bash"]

