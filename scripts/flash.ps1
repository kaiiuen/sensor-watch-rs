# Flash the built firmware ELF to a Sensor-Watch SAM L22 over SWD via probe-rs.
#
# Optional developer tool ONLY. Normal USB drag-and-drop (.uf2) flashing is
# unchanged. This is for when you have an SWD probe on the bench.
#
# Prerequisites:
#   - `cargo install probe-rs-tools` (provides the `probe-rs` binary)
#   - An SWD probe supported by probe-rs (CMSIS-DAP / J-Link) wired to SWDIO +
#     SWCLK + GND pads
#   - A built firmware ELF (cargo build --release --target thumbv6m-none-eabi
#     -p sensor-watch)
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\flash.ps1

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
# Compatibility launcher; probe-rs orchestration lives in the Rust host CLI.
cargo run -p sensor-watch-tools -- flash
