#!/usr/bin/env sh
# Compatibility launcher; build/recovery logic lives in the Rust host CLI.
set -e
cd "$(dirname "$0")"
cargo run -p sensor-watch-tools -- build
