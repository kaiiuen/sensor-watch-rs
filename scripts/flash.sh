#!/usr/bin/env sh
# Flash the built firmware ELF to a Sensor-Watch SAM L22 over SWD via probe-rs.
#
# Optional developer tool ONLY. Normal USB drag-and-drop (.uf2) flashing is
# unchanged and uses build.sh. This is for when you have an SWD probe on the
# bench and want to flash the same release firmware directly.
#
# Prerequisites:
#   - `cargo install probe-rs-tools` (provides the `probe-rs` binary)
#   - An SWD probe supported by probe-rs (e.g. a CMSIS-DAP / J-Link) wired to
#     the board's SWDIO + SWCLK + GND pads
#   - A built firmware ELF (run ./build.sh or `cargo build --release
#     --target thumbv6m-none-eabi -p sensor-watch` first)
#
# Usage: ./scripts/flash.sh
# Output: flashes target/thumbv6m-none-eabi/release/sensor-watch to the target.

set -e

cd "$(dirname "$0")/.."

ELF="target/thumbv6m-none-eabi/release/sensor-watch"

if [ ! -f "$ELF" ]; then
    echo "error: firmware ELF not found at $ELF" >&2
    echo "Build it first with: cargo build --release --target thumbv6m-none-eabi -p sensor-watch" >&2
    exit 1
fi

if ! command -v probe-rs >/dev/null 2>&1; then
    echo "error: probe-rs not found" >&2
    echo "Install it with: cargo install probe-rs-tools" >&2
    exit 1
fi

echo "==> Flashing $ELF to SAM L22J18A over SWD..."

# `probe-rs run` flashes the ELF, resets, and opens a (paused) RTT console.
# Use --no-flash to skip flashing if you only want to attach, or `probe-rs
# download --chip ATSAML22J18A --format elf "$ELF"` to flash without resetting.
probe-rs run --chip ATSAML22J18A --protocol swd --connect-under-reset "$ELF"

echo "==> Done."