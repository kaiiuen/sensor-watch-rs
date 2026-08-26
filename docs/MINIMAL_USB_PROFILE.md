# Developer-only minimal USB feasibility profile

This profile is opt-in and replaces the normal application image. It does not
alter the bootloader, the application origin, the EEPROM emulation region, or
the default production build.

This profile is a minimal application feasibility boundary only. It retains
startup, watchdog, reset/fault recording, device identity, and the normal
bootloader application range while omitting the production application and
optional drivers. It does not activate USB hardware, implement enumeration,
or provide CDC bulk transfer, a terminal, or a shell claim.

## Checks and size

Run from the repository root. Use a repository-local validation target
directory and do not flash until hardware review is complete:

```powershell
$env:CARGO_TARGET_DIR = 'target/validation/minimal-usb'
cargo fmt --all -- --check
cargo test -p sensor-watch --test minimal_profile --features minimal-usb
cargo check --target thumbv6m-none-eabi -p sensor-watch --bin minimal-usb --features minimal-usb
cargo build --release --target thumbv6m-none-eabi -p sensor-watch --bin minimal-usb --features minimal-usb
cargo size --release --target thumbv6m-none-eabi -p sensor-watch --bin minimal-usb --features minimal-usb
```

The release ELF is:

```text
target/validation/minimal-usb/thumbv6m-none-eabi/release/minimal-usb
```

The image must fit the application region `0x00002000..0x0003C000`, or
`0x3A000` bytes. Report the exact ELF path and byte size from the size check.
USB register and packet-memory work is deliberately excluded from this
profile and must be reviewed separately before any hardware implementation.

## Hardware validation required

No hardware USB pass is implied by these builds. A future implementation must
first provide a reviewed PAC/raw SRAM mapping and then test, on a board with
battery removed or otherwise protected from USB back-powering:

1. VBUS connect and disconnect without disturbing reset or WDT behavior.
2. Full-speed reset, GET_DESCRIPTOR, SET_ADDRESS, and SET_CONFIGURATION.
3. Suspend, resume, bus reset, unplug, and repeated reconnect behavior.
4. EP0 packet correctness with a USB protocol analyzer or host trace.
5. Only after that, real CDC bulk host terminal read/write and a separately
   reviewed command policy.

The existing UF2 bootloader remains the only supported flash entry path. Preserve
a known-good production UF2 before replacing the application image. Restore it
through the normal bootloader gesture after testing. Do not use this profile in
Studio or package it as production firmware.
