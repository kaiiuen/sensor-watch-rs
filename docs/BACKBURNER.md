# Backburner Ideas

Ideas that are interesting but not currently being worked on. They're captured
here so they aren't lost. Each has a brief description and the reason it's
deferred.

---

## 1. Companion App (Firmware Studio)

A dedicated desktop app that acts as an **editor, debugger, and assembler** for
the firmware. It would:

- Include all source code and documentation
- Let you assemble watch faces
- Edit, debug, and assemble the code
- Produce the final `.uf2` firmware file

**Status:** Backburner. This is the end-goal app. The firmware itself is now
solid; the app is the next major project.

---

## 2. Real-Time Clock Calibration (without manual setting)

The idea is to calibrate the watch's clock to real time without the user setting
it manually.

### The problem

The watch is air-gapped. It has no network. Setting the time requires manual
input. The flash transfer speed is unreliable for precise timing.

### The proposed approach

1. The PC program knows the current time.
2. It prepares a small file with the current time (everything except seconds),
   targeting the **next minute boundary**.
3. The program waits on the PC.
4. When the next minute hits, it sends a tiny file to the watch.
5. The watch reads it instantly and resets its time to the exact minute.

### Why build-time won't work

The build time is baked into the `.uf2` at compile time. Unless the `.uf2` is
compiled **on demand** by the end user (right before flashing), the build time
is stale by the time it's flashed. So build-time calibration is unreliable.

**Status:** Backburner. Requires the USB/serial shell to be implemented first
(the watch needs a way to receive the file from the PC).

---

## 3. Drift Calibration (from master-clock)

The master-clock project measures crystal drift (parts-per-million) and applies
a frequency correction. The Sensor Watch has an RTC frequency-correction
register (`freqcorr_write`) that could compensate for crystal drift.

**Status:** Backburner. Would be a nice timekeeping improvement, but requires
careful measurement and is not critical.

---

## 4. Raise-to-Wake (accelerometer)

Show seconds only when the user raises their wrist to look at the watch, then
hide them again. Requires an accelerometer on the 9-pin connector (optional
hardware).

**Status:** Backburner. The base watch has no accelerometer. The manual seconds
toggle (bottom-right button) works on all boards. The accelerometer driver and
tap framework are now in place, so this is feasible if an accelerometer is
installed.

---

## 5. BACKUP-Mode "Power Off" Feature

A menu option that puts the watch into its deepest sleep (BACKUP mode) when not
worn. Before entering BACKUP, save state to flash; on wake, restore it.

**Status:** Backburner. STANDBY is the primary mode (retains RAM + display).
BACKUP would be an optional extreme-power-save feature.

---

## 6. Watchdog / Heartbeat Survival

A design where the watch tracks its own uptime or status via a heartbeat from
the RTC's ticking seconds. If the seconds stop updating (a hang), the watchdog
restarts the watch. The goal is to guarantee the watch always recovers from a
freeze, since software freezes are almost always the cause.

**Status:** Backburner. The hardware watchdog already resets on a hang; a
software heartbeat would add a second layer of detection and reporting.

---

## 7. Dual-Boot / Self-Healing Partitioning

Split the 256 KB flash into a minimal protected Golden Image bootloader and an
Application slot. If the application fails its CRC check (bit-rot), the Golden
Image forces the watch into a safe recovery display instead of hard-crashing.

**Status:** Backburner. The CRC-32 integrity check is implemented; the dual-boot
partitioning and recovery image are not.

---

## 8. Log-Structured Wear Leveling + ECC

Replace the simple 8-row rotation with a log-structured ring buffer across the
whole 8 KB EEPROM area, and add SECDED (single-error-correct,
double-error-detect) Hamming codes to every 32-bit chunk to correct bit errors
in flash.

**Status:** Backburner. The simple wear leveling and write-verify are
implemented; the log-structured ring and ECC are not.

---

## 9. Clock Failure Detector (CFD)

Enable the SAM L22's hardware Clock Failure Detector. If the 32 kHz crystal
stops, the CFD switches the time base to the internal OSCULP32K and the watch
falls back to a slightly less accurate clock instead of freezing.

**Status:** Backburner. The RTC relies on the external crystal; the CFD fallback
is not yet wired.

---

## 10. USB / Serial Shell

A USB or serial command shell so the watch can receive files and commands from
a PC. This is a prerequisite for clock calibration and for the companion app.

**Status:** Backburner. Required for clock calibration and deeper PC
integration.

---

## 11. BLE / Companion Connectivity

Bluetooth Low Energy connectivity for configuration and data transfer with a
phone or computer.

**Status:** Backburner. The Sensor Watch hardware does not include BLE; it would
require an external module on the 9-pin connector.

---

## 12. Benchmarks, Fuzzing, and Structured Logging

- Performance benchmarks for interrupt latency and power consumption.
- Fuzz testing for button events and RTC input.
- `defmt`/RTT structured logging for debugging without breaking real-time
  behavior.

**Status:** Backburner. Nice-to-have tooling once the firmware is feature-complete.
