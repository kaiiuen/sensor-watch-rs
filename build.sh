#!/usr/bin/env sh
# Builds the firmware and produces a .uf2 file for drag-and-drop flashing.
#
# Usage: ./build.sh
# Output: target/thumbv6m-none-eabi/release/sensor-watch.uf2

set -e

cd "$(dirname "$0")"

echo "==> Building firmware (release)..."
cargo build --release --target thumbv6m-none-eabi -p sensor-watch

ELF="target/thumbv6m-none-eabi/release/sensor-watch"
BIN="target/thumbv6m-none-eabi/release/sensor-watch.bin"
UF2="target/thumbv6m-none-eabi/release/sensor-watch.uf2"

echo "==> Extracting raw binary..."
# Locate rust-objcopy (bundled with the Rust toolchain).
OBJCOPY=$(find "$HOME/.rustup/toolchains" -name "rust-objcopy.exe" -o -name "rust-objcopy" 2>/dev/null | head -n 1)
if [ -z "$OBJCOPY" ]; then
    echo "error: rust-objcopy not found" >&2
    exit 1
fi
"$OBJCOPY" -O binary "$ELF" "$BIN"

echo "==> Converting to UF2..."
# Run the tool on the host target (not the embedded target).
cargo run -p sensor-watch-core --bin uf2tool --target x86_64-pc-windows-msvc -- "$BIN" "$UF2"

echo "==> Done: $UF2"
ls -la "$UF2"
