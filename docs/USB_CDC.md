# USB CDC application mode

The reference Sensor-Watch firmware uses TinyUSB CDC on the SAM L22 at full
speed. Its application descriptors use vendor/product `0x1209:0x2151`, 64-byte
packets, notification endpoint `0x81`, bulk OUT endpoint `0x02`, and bulk IN
endpoint `0x82`. The Rust shell has a feature-gated transport boundary for
that mode in `src/watch/shell.rs` and the descriptor constants are retained in
`src/watch/usb.rs`.

## Current status

Native CDC is **not yet supported**. The `atsaml22j` `0.1.0` PAC exposes the
SAM L22 USB device control/status
register surface, but does not expose the controller's descriptor/endpoint
transfer SRAM interface required to install descriptors and service endpoint
buffers. The workspace dependency graph also contains no TinyUSB port and no
reviewed compatible Rust USB device stack. Implementing transfers by guessing
the SRAM mapping, or by treating register presence as a complete device API,
would be unsafe and could present a nonfunctional device as working.

A CDC application mode is technically feasible without changing the UF2
bootloader, because the application has its own USB device address space and the
existing shell already has a feature-gated transport boundary. It is not yet a
working reference implementation: the missing PAC/HAL coverage and device stack
must be supplied and reviewed first. The opt-in feature is currently
compile-safe scaffolding, not a CDC claim:

```sh
cargo check --target thumbv6m-none-eabi -p sensor-watch
cargo check --target thumbv6m-none-eabi -p sensor-watch --features usb-cdc
cargo test --lib --features usb-cdc
```

The host test checks the reviewed descriptor/endpoint contract. It does not
emulate USB and must not be interpreted as CDC functionality. A host
`cargo check --features usb-cdc` is expected to fail at the compile-time
feature guard because the firmware feature is ARM-only.

The default build does not enable USB, preserving the battery-safe 4 MHz clock
and the existing UF2 application/bootloader split. If `usb-cdc` is enabled,
firmware initialization returns an explicit `UsbError::Unsupported` and stops
rather than silently running a partial USB implementation. Completing this
feature requires a PAC update or a reviewed SAM L22 USB SRAM HAL plus a device
stack, followed by transfer, host enumeration, suspend/resume, and power tests.
