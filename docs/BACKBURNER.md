# Backburner Ideas

Ideas that are interesting but not currently being worked on. They're captured
here so they aren't lost. Each has a brief description and the reason it's
deferred.

Below, items that are now **implemented** are recorded in a "DONE / implemented"
section before the active backburner, so the history is kept without cluttering
the list of what is still open.

---

## Done / implemented (cleared from the active backlog)

These were previously on the backburner and are now built and working. They are
kept here for the record only.

### Firmware Studio companion app

A dedicated desktop app (editor, debugger, and assembler) for the firmware. It
includes the source and docs, assembles watch faces, and produces the final
`.uf2` firmware file.

**Status:** Done. `studio/` is built out (panels, simulator, build-to-UF2, time
sync). See `docs/README.md` and `studio/README.md`.

### Clock calibration via the shell

Set the watch's clock to real time from a PC without manual entry. The serial
shell provides `settime YYMMDDHHMMSS`, so a PC can set the time precisely.

**Status:** Done. `settime YYMMDDHHMMSS` is implemented in `src/watch/shell.rs`.

### Drift calibration

Apply a crystal frequency correction to the RTC.

**Status:** Done. The RTC frequency-correction register is exposed
(`freqcorr_write` / `freqcorr_read` in `src/watch/rtc.rs`), and the shell's
`drift N` command applies a correction step.

### Guided calibration

Provide directed clock and drift calibration instead of requiring manual shell
commands.

**Status:** Done. Studio provides guided clock calibration at the next minute
boundary and guided drift calibration. The hardware path still requires a UART
jig; see `docs/HARDWARE_ACCESS.md`.

### Raise-to-wake (accelerometer)

Show seconds when the user raises their wrist, then return to the power-saving
rate.

**Status:** Done. A tap / accelerometer wake temporarily shows seconds for a few
seconds even when seconds are hidden, then returns to the power-saving rate.
Requires an accelerometer to be installed.

### BACKUP-mode power off

A menu option that puts the watch into its deepest sleep (BACKUP mode) when not
worn, saving state before entering and restoring it on wake.

**Status:** Done. The diagnostics settings submenu has a POWER OFF option that
saves settings and enters BACKUP mode.

### Watchdog / heartbeat

A design where the watch tracks its own uptime via a heartbeat from the RTC's
ticking seconds, so a hang is detected and recovered from.

**Status:** Done. `check_heartbeat()` detects a frozen RTC (seconds not
advancing) and records an `RtcLostTime` fault; the hardware watchdog resets on a
full hang.

### Log-structured wear leveling + ECC

Replace the simple row rotation with a log-structured ring buffer and protect
chunks with SECDED Hamming codes.

**Status:** Done. `wear_leveled_write()` / `wear_leveled_read()` implement
version-magic row handling with crash recovery, and `ecc_write()` / `ecc_read()`
provide SECDED-protected storage.

### Clock failure detector (CFD)

Enable the SAM L22's hardware Clock Failure Detector so that if the 32 kHz
crystal stops, the RTC switches to the internal oscillator.

**Status:** Done. `init_cfd()` enables the CFD with auto-switchback, and
`check_clock_failure()` records a fault if the crystal failed.

### USB / serial shell

A shell command so the watch can receive commands from a PC.

**Status:** Done. `shell.rs` provides a minimal command interpreter over UART
(`time`, `settime YYMMDDHHMMSS`, `drift N`, `help`). Note the shell is only
reachable over the UART jig, not over USB. See `docs/HARDWARE_ACCESS.md`.

### UF2 integrity and recovery staging

A host-side safety layer for checking firmware artifacts before USB flashing,
recording board metadata and CRC/SHA256 values, preserving a known-good UF2,
and staging an explicit rollback copy.

**Status:** Done for software-only recovery. `scripts/verify-uf2.py` validates
UF2 framing, SAM L22 metadata, application size, CRC32, and SHA256. Its
`backup` and `rollback` commands are non-destructive staging helpers. This does
not provide a golden-image bootloader, true dual boot, automatic device-side
rollback, or any replacement of the ROM bootloader.

---

## Active backburner

### 1. Benchmarks, fuzzing, and structured logging

- Performance benchmarks for interrupt latency and power consumption.
- Fuzz testing for button events and RTC input.
- `defmt`/RTT structured logging for debugging without breaking real-time
  behavior.

**Status:** Partially done. A benchmark/self-test (ECC + CRC) is available in
the diagnostics face, and the fixed-size RAM event log is implemented. Fuzzing
and `defmt`/RTT transport are not yet implemented; the event log is not
persistent. A hardware test plan lives in `docs/TESTING.md`.

### 2. Dual-boot / self-healing partitioning

Split the flash into a minimal protected Golden Image bootloader plus an
Application slot. If the application fails its CRC check, force a safe recovery
display instead of running corrupted code.

**Status:** Partially done. The CRC-32 integrity check is implemented as a
non-bricking check; it does not call `recovery_halt()` or provide device-side
recovery. True dual boot with a separate Golden Image bootloader partition
remains unavailable.

### 3. USB serial console (CDC)

Expose a virtual serial port over USB so the shell is reachable over the cable
rather than a UART jig.

**Status:** Blocked. The watch's USB is file-transfer-only: the UF2 bootloader
lives in the SAM L22's ROM boot region (`0x0000_0000`-`0x0000_2000`), separate
from the firmware (which starts at `0x0000_2000`). Serial-over-USB (CDC) is not
possible without replacing that ROM bootloader, which is out of scope. The real
access paths are a UART jig and an SWD probe; see `docs/HARDWARE_ACCESS.md`.

### 4. Configurable boot / OTA deployment tools

Polish and harden the deployment story around the UF2 bootloader:

- Let the companion app orchestrate a multi-step flash, verify, and reboot loop.
- Detect a stuck or failed flash and fall back cleanly to a safe state.

**Status:** Partially done. `scripts/verify-uf2.py` now provides host-side
validation, known-good backup preservation, manifest output, and explicit
rollback staging. Studio orchestration, USB-device detection, reboot
verification, and automatic fail-safe loops remain unimplemented. There is no
true golden-image bootloader or device-side rollback support.