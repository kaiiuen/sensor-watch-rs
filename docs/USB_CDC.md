# USB CDC and minimal enumeration status

The production reference uses TinyUSB commit
`5572168994a29266df6cbf12b46919498d3ece66` on the SAM L22 at full speed with
VID:PID `0x1209:0x2151`, 64 byte EP0 packets, notification endpoint `0x81`,
bulk OUT endpoint `0x02`, and bulk IN endpoint `0x82`.

## Implemented

The opt-in `minimal-usb` feature implies `usb-enum` and selects the separate
`minimal-usb` firmware binary. Production firmware and its default clocks,
watch application, bootloader range, EEPROM bounds, WDT, and battery behavior
are unchanged.

The minimal image contains a reviewed, bounded EP0 feasibility path, but physical EP0 transfers are not proven and CDC is not enabled:

- USBCRM DFLL48M setup from the documented 32 kHz reference and GCLK1 to USB
- USB AHB and APBB clocks
- PA24/PA25 output-low preconditioning and mux G for DM/DP
- PA05 VBUS detect input for the OSO-SWAT-A1-05 board, explicitly enabled as a pulled-down digital input
- USB reset, full-speed device mode, descriptor address, and EP0 control setup
- eight endpoint groups with two 16 byte banks per group and 32 byte stride
- setup reception and bounded GET_DESCRIPTOR for device and configuration data
- SET_ADDRESS, GET_CONFIGURATION, and SET_CONFIGURATION
- status zero length packets, stalls, reset handling, suspend detach, and
  bounded EP0 re-arm

The EP0 code is implemented as a bounded hardware feasibility path, not a
claim of successful USB enumeration. The CDC transport remains replaceable and
inert.

No CDC bulk endpoint is configured. `CdcTransport` provides fixed 64-byte
buffers, explicit connection and USB states, line-coding and control-request
contracts, and a read-only command allowlist (`ping`, `help`, `time`,
`identity`). Until physical EP0 and CDC transfers are proven, transport
operations return `NotEnumerated` or `Unsupported`. No shell response is
fabricated and no mutating command is accepted.

## Hardware-unverified details

The software checks and host tests cover the USB register offsets, endpoint
bank field layout, SRAM section contract, clock IDs, USB pad mux, OTP5 address,
and OTP5 PADCAL and
DFLL calibration fields. The minimal path reads those documented OTP5 fields
and writes PADCAL. Electrical signal quality, DFLL lock behavior, and actual
host enumeration still require hardware verification.

The VBUS input mapping (PA05) is taken from the A1-05 board reference, not from
the MCU device pack. The input buffer and pull-down are explicitly configured;
a different board revision must not use this image without a separate VBUS
review. USB disconnect, suspend, and no-VBUS handling detach, clear software address/
EP0 state where required, and reinitialize only after VBUS is present; they do
not enable a battery-side power path.

Host control tests:

```text
cargo test -p sensor-watch --lib --features usb-enum
cargo test -p sensor-watch --test minimal_profile
```

ARM checks:

```text
cargo check --target thumbv6m-none-eabi -p sensor-watch --bin minimal-usb --features minimal-usb
cargo build --release --target thumbv6m-none-eabi -p sensor-watch --bin minimal-usb --features minimal-usb
```
