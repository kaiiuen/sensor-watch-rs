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
# Compatibility launcher; probe-rs orchestration lives in the Rust host CLI.
cargo run -p sensor-watch-tools -- flash
