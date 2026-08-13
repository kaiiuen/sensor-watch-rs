# Developer Debugging (SWD / probe-rs)

An on-silicon debug path for developers who have the hardware on the bench.
This is **optional tooling** - normal USB drag-and-drop `.uf2` flashing and the
companion `studio` app are completely unchanged. Everything here is additive.

See [HARDWARE_ACCESS.md](HARDWARE_ACCESS.md) for the background on why the USB
port is file-transfer-only and for the UART shell wiring. This document focuses
on the SWD/probe-rs path and on debugging the firmware itself.

## Why this exists

The watch's USB port only exposes a mass-storage bootloader (you drag a `.uf2`
across and can inspect UF2/file-transfer information). It cannot host a CPU
debugger or UART shell. To set breakpoints, single-step, read registers, or get
a real backtrace on a fault, you need the **SWD** interface
(debug pads on the board) and a debug probe.

## Prerequisites

- **An SWD probe** supported by probe-rs - a CMSIS-DAP style probe (e.g. a
  Raspberry Pi Pico / picoprobe, a DAPLink, or a commercial CMSIS-DAP), or a
  J-Link. It must support the Microchip SAM L22 / Cortex-M0+.
- **Wired to the SWD pads**: SWDIO, SWCLK, and GND. (Some debug headers also
  expose SWO and a reset line, `--connect-under-reset` is used in our scripts to
  handle targets that are hard to halt otherwise.)
- **probe-rs** installed: `cargo install probe-rs-tools` (provides the
  `probe-rs` binary). Install **arm-none-eabi-gdb** too if you want to drive GDB
  directly (see the VSCode section below).
- **A built firmware ELF.** The release firmware is at
  `target/thumbv6m-none-eabi/release/sensor-watch` after running
  `./build.sh` or `cargo build --release --target thumbv6m-none-eabi -p sensor-watch`.

## Flashing via probe-rs (scripts)

Two compatibility scripts launch the Rust host tool's probe-rs command. Both are optional and only touch the release ELF already produced by a normal build.

- `scripts/flash.sh` (POSIX sh / WSL / Git-bash)
- `scripts/flash.ps1` (Windows PowerShell)

They run the equivalent Cargo command:

```sh
cargo run -p sensor-watch-tools -- flash
```

The tool invokes `probe-rs run --chip ATSAML22J18A --protocol swd --connect-under-reset`.

`probe-rs run` flashes the ELF, resets the chip, and starts executing while
opening a (paused) RTT console - the closest thing probe-rs has to a "flash and
go". If you only want to write the image without running, use:

```sh
probe-rs download --chip ATSAML22J18A --format elf \
    target/thumbv6m-none-eabi/release/sensor-watch
```

The scripts check that the ELF exists and that `probe-rs` is on your `PATH`.

> **Caveat:** the firmware links at `0x0000_2000` (it sits after the USB
> bootloader, see `memory.x`). probe-rs flashes at the ELF's declared load
> addresses, so this correctly writes only the application region and leaves
> the bootloader intact - just as `build.sh`'s `.uf2` does.

## Attaching a debugger

For breakpoints, stepping, and backtraces, run a GDB server and attach:

```sh
# Terminal 1: start the SWD GDB server (default port 3333)
probe-rs gdb-server --chip ATSAML22J18A --protocol swd --connect-under-reset

# Terminal 2: connect GDB to the loaded ELF
arm-none-eabi-gdb target/thumbv6m-none-eabi/release/sensor-watch \
    -ex "target remote 127.0.0.1:3333" \
    -ex "load" \
    -ex "break main" \
    -ex "continue"
```

`arm-none-eabi-gdb` disassembles nicely against the ELF's DWARF debug info, so
you get real `file:line` stepping and readable backtraces.

### VSCode

A ready-made [`.vscode/launch.json`](.vscode/launch.json) is included with two
configurations:

1. **probe-rs extension** ("Flash + Debug SAM L22"): install the
   `probe-rs.probe-rs-debugger` extension and the `probe-rs-debugger` helper
   (`cargo install probe-rs-debugger`). This is the simplest full flash+debug
   experience.
2. **cortex-debug** ("Attach via GDB"): install the `marus25.cortex-debug`
   extension. This config starts `probe-rs gdb-server` itself (external
   server), so you do not need OpenOCD.

Both target the SAM L22 (`ATSAML22J18A`) over SWD, use the release ELF, and
halt at `main`.

## The panic fingerprint

The firmware is `#no_std`, so on panic it cannot print a `file:line` directly.
Instead the panic handler (`src/panic.rs`) computes a deterministic **24-bit
fingerprint** of the panic location:

- FNV-1a hash of the source file path, with the **line** number folded in
  (bit-reversed) and the column spread across the top bits.

It stores the fingerprint in the RTC backup registers (`src/movement/fault.rs`,
via `record_panic_fingerprint` / `panic_fingerprint`) **before** resetting, so it
survives the reboot. The LED blinks as a visible fault indicator, then the
device resets and returns to normal operation.

### Reading a fingerprint over the UART shell

After a panic-and-reset, connect the UART shell (HARDWARE_ACCESS.md, 9600 baud,
SERCOM3 on pads A4/A2/GND) and type:

```
panic
```

The shell replies with `P` followed by 6 hex digits, e.g. `P3fa862`. That is
the 24-bit fingerprint of the panic `file:line`.

### Mapping fingerprint -> file:line:column

Firmware Studio's **Bugs** panel accepts the `Pxxxxxx` value from the shell and
scans the firmware Rust source tree for matching `file:line:column` candidates.
A successful Studio build writes `sensor-watch.panic-map.json` beside the ELF.
That manifest records the SHA-256 of the ELF and of every `src/**/*.rs` and
`core/src/**/*.rs` path/content pair, so the resolver refuses a missing,
replaced, or source-mismatched build rather than producing a misleading match.
The resolver uses the exact firmware algorithm: FNV-1a with offset basis
`0x811c9dc5` and prime `0x01000193`, XOR with `line.reverse_bits()`, XOR with
`column * 2654435761`, then the stored low 24 bits. It scans the source tree
associated with the workspace containing the release ELF:

```
target/thumbv6m-none-eabi/release/sensor-watch
```

Run Studio from the workspace or next to its build output so it can discover the
same firmware directory, then resolve after building that ELF in Studio. A
missing manifest or build/source mismatch is reported explicitly. A validated
no-match result means the fingerprint may not have been produced by this
firmware version. Multiple matches are shown when the 24-bit truncation is
ambiguous. The resolver is host-side only and does not require hardware.

The ELF's DWARF symbols remain the authoritative option for instruction-level
debugging and backtraces over SWD, the resolver is intended for the stored panic
fingerprint reported by the UART shell.

## Quick feedback loop with the UART shell

For fast iteration you usually do **not** need a debugger at all. The firmware
has a minimal command shell over the UART (9600 baud, 8-N-1, SERCOM3, pads
A4/A2/GND). The USB bootloader drive is not an alternate serial path.

A host that shows no serial port does not prove that UART is absent. The UART
jig may be disconnected, powered off, miswired, using an incompatible adapter,
or hidden by the host or a device-passthrough setup. Record this condition as
**NOT AVAILABLE** until the physical and host path has been checked.

The shell commands fall into two safety groups:

- Read-only: `help`, `time`, `drift`, `panic`, `events`, and `optical` when the
  optional command is available.
- Mutating: `settime YYMMDDHHMMSS`, `drift N`, and `events clear`.

Use read-only commands for initial connection checks. Confirm the target and
values before sending a mutating command. Power down before changing wiring,
use a 3.3 V-compatible USB-serial adapter, cross TX and RX, and connect GND.
Never apply 5 V UART signaling to the watch.

- `time` - report RTC time
- `settime YYMMDDHHMMSS` - set the clock
- `drift N` - drift-correction step
- `panic` - report the stored panic fingerprint (see above)
- `help` - list commands

See [HARDWARE_ACCESS.md](HARDWARE_ACCESS.md) for wiring and `src/watch/shell.rs`
for the code. This is the fastest way to get textual feedback from the running
device without any debug tooling.

## Structured event breadcrumbs

The firmware keeps a small, RAM-only structured event ring in
`src/watch/event_log.rs`. It has fixed storage (16 entries, no heap), retains the
newest entries when full, and records a sequence, packed RTC timestamp (or
untimed fallback during early boot/panic), stable event code, and small payload.
Faults are recorded automatically alongside the persistent fault summary.

The optional `defmt-log` Cargo feature mirrors each event to an RTT backend for
an SWD probe. It also emits fault codes, reset reasons, and panic fingerprints.
the persistent backup-register summary and the event ring remain in place as
fallbacks. Enable it only for an ARM firmware build:

```sh
cargo build --target thumbv6m-none-eabi -p sensor-watch --features defmt-log
```

Use the resulting ELF with `probe-rs run` as in the flashing section above. The
normal build command does not enable this feature, so it does not add `defmt`,
RTT, RTT sections, or logging call-site code to the default firmware. The
feature is target-checked and intentionally rejected for host builds.

Over the UART shell:

```text
events
EV 00000000 00000000 01 0002
events clear
```

The fields are sequence, timestamp, event code, and payload, all hexadecimal.
This is deliberately a local breadcrumb buffer rather than a persistent log.
reset clears it. RTT/defmt is a separate live stream and is not a replacement
for the fallback ring or the reset-surviving fault/fingerprint registers.

## Probe/Test result reporting

Studio's current Diagnostics panel is an offline simulator. Its report must not
be treated as a physical probe result, even when a UART jig is connected. For
any Probe/Test or hardware validation report, use these labels exactly:

- **PASS** - the check ran and the acceptance condition was observed.
- **FAIL** - the check ran and the acceptance condition was not observed.
- **NOT AVAILABLE** - the required transport, probe, or hardware is unavailable.
- **NOT TESTED** - the check was not run, so its state is unknown.

A simulated software PASS is not a physical hardware PASS. A missing serial port
or disconnected UART jig is **NOT AVAILABLE**, not FAIL. Use **NOT TESTED** when
no attempt was made.

## Summary

- Optional, purely additive dev tooling: `sensor-watch-tools`, `scripts/flash.sh`, `scripts/flash.ps1`,
  `.vscode/launch.json`, and this document.
- Normal `.uf2` USB flashing and the `studio` app are untouched, `build.sh` and
  the flash scripts remain thin compatibility launchers.
- To debug on silicon you need: an SWD probe + probe-rs, plus (for VSCode) the
  extension(s) of your choice.