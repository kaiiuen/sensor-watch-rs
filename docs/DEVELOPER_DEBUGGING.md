# Developer Debugging (SWD / probe-rs)

An on-silicon debug path for developers who have the hardware on the bench.
This is **optional tooling** — normal USB drag-and-drop `.uf2` flashing and the
companion `studio` app are completely unchanged. Everything here is additive.

See [HARDWARE_ACCESS.md](HARDWARE_ACCESS.md) for the background on why the USB
port is file-transfer-only and for the UART shell wiring. This document focuses
on the SWD/probe-rs path and on debugging the firmware itself.

## Why this exists

The watch's USB port only exposes a mass-storage bootloader (you drag a `.uf2`
across). It cannot host a CPU debugger. To set breakpoints, single-step, read
registers, or get a real backtrace on a fault, you need the **SWD** interface
(debug pads on the board) and a debug probe.

## Prerequisites

- **An SWD probe** supported by probe-rs — a CMSIS-DAP style probe (e.g. a
  Raspberry Pi Pico / picoprobe, a DAPLink, or a commercial CMSIS-DAP), or a
  J-Link. It must support the Microchip SAM L22 / Cortex-M0+.
- **Wired to the SWD pads**: SWDIO, SWCLK, and GND. (Some debug headers also
  expose SWO and a reset line; `--connect-under-reset` is used in our scripts to
  handle targets that are hard to halt otherwise.)
- **probe-rs** installed: `cargo install probe-rs-tools` (provides the
  `probe-rs` binary). Install **arm-none-eabi-gdb** too if you want to drive GDB
  directly (see the VSCode section below).
- **A built firmware ELF.** The release firmware is at
  `target/thumbv6m-none-eabi/release/sensor-watch` after running
  `./build.sh` or `cargo build --release --target thumbv6m-none-eabi -p sensor-watch`.

## Flashing via probe-rs (scripts)

Two small scripts wrap the probe-rs flash command. Both are optional and only
touch the release ELF already produced by a normal build.

- `scripts/flash.sh` (POSIX sh / WSL / Git-bash)
- `scripts/flash.ps1` (Windows PowerShell)

They run:

```sh
probe-rs run --chip ATSAML22J18A --protocol swd --connect-under-reset \
    target/thumbv6m-none-eabi/release/sensor-watch
```

`probe-rs run` flashes the ELF, resets the chip, and starts executing while
opening a (paused) RTT console — the closest thing probe-rs has to a "flash and
go". If you only want to write the image without running, use:

```sh
probe-rs download --chip ATSAML22J18A --format elf \
    target/thumbv6m-none-eabi/release/sensor-watch
```

The scripts check that the ELF exists and that `probe-rs` is on your `PATH`.

> **Caveat:** the firmware links at `0x0000_2000` (it sits after the USB
> bootloader; see `memory.x`). probe-rs flashes at the ELF's declared load
> addresses, so this correctly writes only the application region and leaves
> the bootloader intact — just as `build.sh`'s `.uf2` does.

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

### Mapping fingerprint -> file:line (caveat)

**There is no symbols table yet.** `probe-rs` and debugger **cannot** map the
fingerprint back to a source location, because that mapping is a build-time
table (from the hash of each `panic!` site) that has **not been built**. Right
now the fingerprint only tells you *which* panic site fired, not where it maps
to.

To correlate manually, you must reimplement the FNV-1a hash (offset basis
`0x811c9dc5`, prime `0x01000193`) over each candidate `file:line` and look for a
match. This is exactly what a future build-time tools step should generate:

- a `build.rs` (or a small host tool) that walks the sources, computes the
  fingerprint of every `panic!` location, and emits a `fp -> file:line` table;
- the table embedded in the firmware and/or printed via the `panic` shell
  command.

Until that exists, treat the fingerprint as a stable but **unresolved**
identifier. The debugger's DWARF symbols (attaching over SWD as above) is the
authoritative way to find a panic's real source location today.

## Quick feedback loop with the UART shell

For fast iteration you usually do **not** need a debugger at all. The firmware
has a minimal command shell over the UART (9600 baud, SERCOM3, pads A4/A2/GND):

- `time` — report RTC time
- `settime YYMMDDHHMMSS` — set the clock
- `drift N` — drift-correction step
- `panic` — report the stored panic fingerprint (see above)
- `help` — list commands

See [HARDWARE_ACCESS.md](HARDWARE_ACCESS.md) for wiring and `src/watch/shell.rs`
for the code. This is the fastest way to get textual feedback from the running
device without any debug tooling.

## Optional / not yet done

- A `fp -> file:line` symbols table (see *caveat* above).
- RTT (`rtt_target`) logging in place of (or alongside) the UART shell, to get
  streamed logs over the SWD probe without a second UART dongle.

## Summary

- Optional, purely additive dev tooling: `scripts/flash.sh`, `scripts/flash.ps1`,
  `.vscode/launch.json`, and this document.
- Normal `.uf2` USB flashing and the `studio` app are untouched (`build.sh`,
  CI, and the default `cargo` runner are unchanged).
- To debug on silicon you need: an SWD probe + probe-rs, plus (for VSCode) the
  extension(s) of your choice.