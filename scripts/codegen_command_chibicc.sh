#!/bin/bash
cd /opt/c2rust_agent
cargo run -- translate --input-dir translate_chibicc/src --output-dir /tmp/translate_chibicc --debug
