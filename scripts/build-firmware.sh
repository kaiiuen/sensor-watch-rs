#!/usr/bin/env sh
# Build the firmware from any caller directory, including with --manifest-path.
# Cargo discovers config from the current directory and its ancestors, not from
# the manifest, so pass the existing workspace config when called externally.
set -e

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ "$PWD" = "$repo_dir" ] || case "$PWD/" in "$repo_dir/"*) true;; *) false;; esac; then
    exec cargo build \
        --manifest-path "$repo_dir/Cargo.toml" \
        --package sensor-watch \
        --bin sensor-watch \
        --release \
        --target thumbv6m-none-eabi \
        "$@"
fi

exec cargo --config "$repo_dir/.cargo/config.toml" build \
    --manifest-path "$repo_dir/Cargo.toml" \
    --package sensor-watch \
    --bin sensor-watch \
    --release \
    --target thumbv6m-none-eabi \
    "$@"
