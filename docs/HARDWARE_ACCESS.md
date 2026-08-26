# Talking to the Board (Serial / Debug)

A practical guide for people with a soldering iron and a USB-serial or SWD
probe who want to talk to the firmware on the bench. This is a hardware item
and stays on the backburner: none of it is needed for normal use, and it
requires access to the debug pads on the board.

## The key constraint: USB is file-transfer-only

The watch's USB port does **not** carry a serial console. It is a plain USB
Mass Storage device that lets you drag a `.uf2` file across to flash the
firmware, and nothing else.

The reason is how this SAM L22 board is booted:

- The **UF2 bootloader** lives in the boot region at `0x0000_0000`-`0x0000_2000`
  (the ROM boot region, the first 8 KB of the memory map).
- The **firmware** starts at `0x0000_2000` and runs from there.
- The USB stack presented to the PC is owned by that bootloader, and it only
  implements file transfer. It does not expose a USB CDC (virtual serial) port.

So there is no way to get a "USB serial console" out of the existing bootloader.
The current firmware does not implement CDC. A future application-mode CDC
implementation is technically feasible, but requires the missing SAM L22 USB
transfer-SRAM HAL/PAC coverage and a reviewed device stack, see
[USB_CDC.md](USB_CDC.md). The UF2 bootloader remains file-transfer-only.

## What this means in practice

If you plug the watch in via USB, you can only push firmware files and inspect
bootloader file-transfer information such as `INFO_UF2.TXT`. The USB drive does
not provide a UART shell, probe connection, or general application data channel.
To get a real two-way link to the running firmware, you need one of the two
paths below, both of which use pads on the board rather than the USB connector.

A missing serial port in the operating system or in Studio does not prove that
the board has no UART. The jig may be disconnected, unpowered, wired to the
wrong pins, using a charge-only or non-serial adapter, or not passed through to
the host. Check the physical connection and adapter before treating the result
as a hardware finding.

## Path A: UART jig (serial shell)

The firmware has a minimal command shell over a UART. You connect a USB-serial
dongle to the three debug pads and talk to the shell.

### Wiring the jig

The shell runs on SERCOM3. The pads you need:

- **A2** - UART TX (watch -> dongle RX)
- **A3** - UART RX (dongle TX -> watch)
- **GND** - common ground

Wire them cross-over to the dongle (A2 to its RX, A3 to its TX, and GND to
GND), then open a terminal at **9600 baud**, 8-N-1. Use a 3.3 V-compatible
USB-serial adapter. Do not connect a 5 V UART signal to the watch.

Power down before changing wires. Verify TX/RX direction, common ground, and
that the adapter is not supplying unintended power. Keep the USB cable path
separate from the UART signal path: the USB connector remains a UF2
file-transfer drive, not the shell connection.

The console is a small event-driven interpreter, RX is nonblocking and buffered
in a bounded ring. An overrun returns `ERR rx-overflow`, and lines longer than 32
bytes return `ERR line-too-long` rather than being truncated, see `src/watch/shell.rs` for
the code and `src/watch/uart.rs` for the driver.

### Shell commands

Once connected, type a command and press Enter. The supported commands are:

#### Read-only commands

These query the running firmware and should not change watch state:

- `help`
- `time`
- `drift` without a value
- `optical` when the optional command is built in
- `panic`
- `events`

#### Mutating commands

These change state and require extra care:

- `settime YYMMDDHHMMSS` changes the RTC time.
- `drift N` changes the signed RTC frequency correction.
- `events clear` erases the retained RAM event ring.

Confirm the target and intended values before sending a mutating command. Do
not use a mutating command as a connectivity test. A shell response of `OK`
means the command was accepted by the firmware, it is not a complete
verification of the physical sensor, display, or other hardware.

- `time` - report the current RTC time as `TIME YYMMDDHHMMSS`.
- `settime YYMMDDHHMMSS` - set the clock, replies `OK` on success.
- `drift` - read the signed frequency-correction value.
- `drift N` - set the signed correction (`N` is -127..127).
- `optical` - report OPT3001/optical-sensor status.
- `panic` - report the stored panic fingerprint.
- `events` - dump the retained RAM event ring.
- `events clear` - clear the RAM event ring.
- `help` - list the commands.

Commands use strict ASCII forms: `settime` requires exactly 12 decimal digits,
and drift values must be signed decimal values in the hardware range. Mutating
commands are locked by default in every firmware build. The `shell-auth`
feature must be enabled for the physical Alarm/service-button hook to unlock
mutations. When enabled, they unlock only while that button is held, for at
most 30 seconds, and revoke immediately on release. Without `shell-auth`,
mutations remain locked even when the button is pressed. A UART connection
alone never unlocks mutations. Reads remain available at all times.

This is how clock setting and drift correction can be driven from a PC, for
example by the companion app during calibration. Studio's **Shell Access**
panel exposes this as an explicit **UART Jig** mode: refresh host ports, select
the adapter, connect, and then send commands. The default **Simulated** mode
continues to operate entirely on the in-app watch model. A port-open, write, or
read timeout is shown as an error, it is never reported as a watch response.

The Studio **Probe/Test** workflow is represented by the current **Diagnostics**
panel. Its full diagnostic run is offline and simulated, even when a UART jig
is connected. It must not be described as physical hardware validation. Use the
Shell Access panel for explicit UART observations and SWD/probe-rs for silicon
inspection.

Use these result labels consistently:

- **PASS** - the stated check ran and its acceptance condition was observed.
- **FAIL** - the check ran and its acceptance condition was not observed.
- **NOT AVAILABLE** - the check cannot run in the current setup, such as no
  UART jig, no SWD probe, or missing optional sensor hardware.
- **NOT TESTED** - no conclusion was attempted, so the result is unknown.

A simulated check may be PASS for software behavior, but it is not PASS for
physical hardware. A missing port or disconnected jig should be NOT AVAILABLE,
not FAIL and not proof that UART is absent.

## Path B: SWD probe (full debug)

If you need breakpoints, a backtrace, or register inspection, use the SWD
interface with a debug probe.

- Connect an SWD probe (any probe-rs or CMSIS-DAP / J-Link style probe that
  supports the SAM L22 / Cortex-M0+) to the SWD pads.
- Flash and halt target firmware with `probe-rs run` or `openocd`.
- Set breakpoints, single-step, and read a backtrace on a fault.

This is the path to use when debugging the firmware itself, as opposed to just
talking to the running shell.

> For the ready-made scripts, VSCode configs, prerequisites, and how to read a
> panic fingerprint over the shell, see
> [DEVELOPER_DEBUGGING.md](DEVELOPER_DEBUGGING.md).

## Which one for what task

| Task                       | Path             |
|----------------------------|------------------|
| Set the clock              | A (UART shell)   |
| Apply drift correction     | A (UART shell)   |
| Calibrate against a PC     | A (UART shell)   |
| Debug a hang or fault      | B (SWD)          |
| Breakpoints / backtrace    | B (SWD)          |
| Flash firmware files       | USB (bootloader) |

## Summary

- The UF2 USB path is file-transfer-only today. Native application CDC is not
  implemented, its feasibility and missing HAL/stack work are tracked in
  `docs/USB_CDC.md`. Studio does not pretend that the UF2 drive is a serial port.
- Two real access paths exist: a UART jig (3 debug pads, 9600 baud, the shell
  with `time` / `settime` / `drift N` / `help`) and an SWD probe.
- Both require hardware and soldering, so this stays a backburner / hardware
  item, it is not needed for normal use of the watch.
- See [DEVELOPER_DEBUGGING.md](DEVELOPER_DEBUGGING.md) for the SWD/probe-rs
  flash, debugger attach, and panic-fingerprint workflow.