# USB CDC application mode

The reference Sensor-Watch firmware uses TinyUSB CDC on the SAM L22 at full
speed. Its application descriptors use vendor/product `0x1209:0x2151`, 64-byte
packets, notification endpoint `0x81`, bulk OUT endpoint `0x02`, and bulk IN
endpoint `0x82`. The Rust shell has a feature-gated transport boundary for
that mode in `src/watch/shell.rs` and the descriptor constants are retained in
`src/watch/usb.rs`.

## Current status

Native CDC is **not yet supported**. The `atsaml22j` `0.1.0` PAC exposes the
USB device control and endpoint-status registers, but does not expose the
USB descriptor/endpoint transfer SRAM required by the SAM L22 USB device
controller. This workspace also has no TinyUSB or compatible Rust USB device
stack. Implementing transfers by guessing those addresses would be unsafe and
could present a nonfunctional device as working.

The opt-in feature is therefore compile-safe scaffolding, not a CDC claim:

```sh
cargo build --target thumbv6m-none-eabi -p sensor-watch
cargo build --target thumbv6m-none-eabi -p sensor-watch --features usb-cdc
```

The default build does not enable USB, preserving the battery-safe 4 MHz clock
and the existing UF2 application/bootloader split. If `usb-cdc` is enabled,
firmware initialization returns an explicit `UsbError::Unsupported` and stops
rather than silently running a partial USB implementation. Completing this
feature requires a PAC update or a reviewed SAM L22 USB SRAM HAL plus a device
stack, followed by transfer and host enumeration tests.
