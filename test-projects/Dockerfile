# Copyright (c) 2025 vivo Mobile Communication Co., Ltd.
# problems is licensed under Mulan PSL v2.
# You can use this software according to the terms and conditions of the Mulan
# PSL v2. You may obtain a copy of Mulan PSL v2 at:
#             http://license.coscl.org.cn/MulanPSL2
# THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
# EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
# MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.  See the Mulan PSL v2 for
# more details.

FROM debian:latest

ENV RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static \
    RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup
RUN sed -i 's/deb.debian.org/mirrors.ustc.edu.cn/g' /etc/apt/sources.list.d/debian.sources
RUN apt update && \
    apt install -y build-essential cmake ninja-build bear pkg-config \
        libfuse-dev clang llvm lld python3-pip python3-venv curl cloc && \
    pip config set global.index-url https://mirrors.ustc.edu.cn/pypi/simple && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
