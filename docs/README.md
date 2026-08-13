# Sensor-Watch Firmware - Documentation Index

This directory contains the complete documentation for the Sensor-Watch firmware
rewrite in Rust.

## Documents

| Document | Contents |
|----------|----------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | The high-level architecture: design philosophy, power states, resource budget, boot sequence, and every module. |
| [MODULES.md](MODULES.md) | A symbol-by-symbol reference for every module and public item. |
| [POWER.md](POWER.md) | Power-management deep-dive: states, event-driven model, and the control mechanisms. |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to build, test, and contribute, including how to add a new watch face. |
| [TESTING.md](TESTING.md) | The hardware test plan: power, RTC accuracy, flash wear, faces, faults, and peripherals. |
| [BACKBURNER.md](BACKBURNER.md) | Unfinished work reserved for future implementation. |
| [HARDWARE_ACCESS.md](HARDWARE_ACCESS.md) | How to talk to the board: the USB-is-file-transfer-only constraint, the ROM UF2 bootloader, the UART jig, and the SWD probe. |
| [DEVELOPER_DEBUGGING.md](DEVELOPER_DEBUGGING.md) | Optional on-silicon debugging via SWD / probe-rs for developers with bench hardware. |
| [USB_CDC.md](USB_CDC.md) | Native CDC status: feasible in principle, but currently scaffolding only. |
| [RECOVERY.md](RECOVERY.md) | Host-side UF2 validation, backup, and rollback staging; no device-side dual boot. |

## Companion app

The `studio/` directory contains **Firmware Studio**, the GUI companion app
(editor, debugger, assembler). See [`studio/README.md`](../studio/README.md) for
its panels, features, and dependencies.

## Quick orientation

- **`src/watch/`** - the hardware abstraction layer (HAL). One module per
  peripheral (RTC, LCD, GPIO, buttons, LED, buzzer, ADC, I2C, SPI, UART,
  storage, deep-sleep, watchdog, CRC, memory, LIS2DW, utility).
- **`src/movement/`** - the watchface framework and the faces. The dispatcher
  (`mod.rs`) reacts to events; each face implements the `WatchFace` trait.
- **`core/`** - pure logic (date math, settings bit-packing, UF2 encoding) that
  is host-testable.
- **`src/main.rs`** - the entry point and boot sequence.
- **`src/panic.rs`** - the self-recovering panic handler.

## The one idea to remember

> **The CPU is a start/stop resource. It wakes only to react to a single event,
> then immediately returns to STANDBY. All timekeeping is owned by the RTC,
> never by the CPU.**

Everything in the firmware follows from this.
