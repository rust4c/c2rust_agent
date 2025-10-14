#!/bin/bash
cd /opt/c2rust_agent
cargo run -- translate --input-dir translate_littlefs_fuse/src --output-dir /tmp/translate_littlefs_fuse --debug
