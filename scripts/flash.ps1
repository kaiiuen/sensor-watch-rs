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

$ELF = "target\thumbv6m-none-eabi\release\sensor-watch"

if (-not (Test-Path $ELF)) {
    Write-Error "firmware ELF not found at $ELF; build it with: cargo build --release --target thumbv6m-none-eabi -p sensor-watch"
    exit 1
}

if (-not (Get-Command probe-rs -ErrorAction SilentlyContinue)) {
    Write-Error "probe-rs not found; install it with: cargo install probe-rs-tools"
    exit 1
}

Write-Host "==> Flashing $ELF to SAM L22J18A over SWD..."

probe-rs run --chip ATSAML22J18A --protocol swd --connect-under-reset $ELF

Write-Host "==> Done."