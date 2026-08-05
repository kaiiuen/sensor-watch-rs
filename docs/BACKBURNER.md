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
it manually. The serial shell (item 10) now provides a `settime YYMMDDHHMMSS`
command, so a PC can set the time precisely.

**Status:** Partially done. The serial shell command exists; a full
next-minute-boundary calibration flow (the PC waiting for the exact minute and
sending a precise timestamp) is not yet built into the companion app.

---

## 3. Drift Calibration (from master-clock)

The master-clock project measures crystal drift (parts-per-million) and applies
a frequency correction. The `apply_drift_correction(ppm)` / `get_drift_correction()`
functions now expose the RTC frequency-correction register.

**Status:** Partially done. The drift-correction API exists; an automated
measurement-and-apply loop (measuring drift over time and setting the correction)
is not yet built.

---

## 4. Raise-to-Wake (accelerometer)

Show seconds only when the user raises their wrist to look at the watch, then
hide them again. Requires an accelerometer on the 9-pin connector (optional
hardware).

**Status:** Done. A tap (SingleTap/DoubleTap/AccelerometerWake) temporarily shows
seconds for 5 seconds even when seconds are hidden, then returns to the
power-saving rate. Requires an accelerometer to be installed.

---

## 5. BACKUP-Mode "Power Off" Feature

A menu option that puts the watch into its deepest sleep (BACKUP mode) when not
worn. Before entering BACKUP, save state to flash; on wake, restore it.

**Status:** Done. The diagnostics settings submenu has a POWER OFF option that
saves settings and enters BACKUP mode.

---

## 6. Watchdog / Heartbeat Survival

A design where the watch tracks its own uptime or status via a heartbeat from
the RTC's ticking seconds. If the seconds stop updating (a hang), the watchdog
restarts the watch.

**Status:** Done. `check_heartbeat()` detects a frozen RTC (seconds not
advancing) and records an `RtcLostTime` fault. The hardware watchdog still
resets on a full hang.

---

## 7. Dual-Boot / Self-Healing Partitioning

Split the 256 KB flash into a minimal protected Golden Image bootloader and an
Application slot. If the application fails its CRC check (bit-rot), the Golden
Image forces the watch into a safe recovery display.

**Status:** Partially done. The CRC-32 integrity check is implemented, and on
failure `recovery_halt()` blinks the LED and halts instead of running corrupted
code (the watchdog resets). A true dual-boot with a separate Golden Image
bootloader partition is not implemented.

---

## 8. Log-Structured Wear Leveling + ECC

Replace the simple 8-row rotation with a log-structured ring buffer across the
whole 8 KB EEPROM area, and add SECDED (single-error-correct,
double-error-detect) Hamming codes to every 32-bit chunk.

**Status:** Done. `wear_leveled_write()` now writes a version-magic header per
row and `wear_leveled_read()` scans for the most recent valid entry (crash
recovery). `ecc_write()` / `ecc_read()` provide SECDED-protected storage.

---

## 9. Clock Failure Detector (CFD)

Enable the SAM L22's hardware Clock Failure Detector. If the 32 kHz crystal
stops, the CFD switches the time base to the internal OSCULP32K.

**Status:** Done. `init_cfd()` enables the CFD with auto-switchback, and
`check_clock_failure()` records a fault if the crystal failed.

---

## 10. USB / Serial Shell

A USB or serial command shell so the watch can receive files and commands from
a PC. This is a prerequisite for clock calibration and for the companion app.

**Status:** Done. `shell.rs` provides a minimal command interpreter over UART
(`time`, `settime YYMMDDHHMMSS`, `help`), wired into the app loop.

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

**Status:** Partially done. A benchmark/self-test (ECC + CRC) is available in the
diagnostics face. Fuzz testing and `defmt`/RTT structured logging are not yet
implemented.
