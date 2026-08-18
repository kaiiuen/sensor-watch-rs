# Backburner Ideas

Only unfinished work belongs here. Completed features are removed rather than
archived in this list, implementation history belongs in Git history and the
project's private continuity notes.

---

## Active backburner

### 1. Benchmarks, fuzzing, and structured logging

- Performance benchmarks for interrupt latency and power consumption.
- Fuzz testing for button events and RTC input.
- `defmt`/RTT structured logging for debugging without breaking real-time
  behavior.

**Status:** Partially done. A benchmark/self-test (ECC + CRC) is available in the
diagnostics face, and the fixed-size 16-entry RAM event log is implemented.
The opt-in `defmt-log` feature mirrors events to RTT over SWD, it is not enabled
by default and the event log is not persistent. Face fuzzing and on-silicon
validation remain open. A hardware test plan lives in `docs/TESTING.md`.

### 2. Dual-boot / self-healing partitioning

Split the flash into a minimal protected Golden Image bootloader plus an
Application slot. If the application fails its CRC check, force a safe recovery
display instead of running corrupted code.

**Status:** Partially done. The CRC-32 integrity check is implemented as a
non-bricking check, it does not provide device-side recovery. True dual boot
with a separate Golden Image bootloader partition remains unavailable.

### 3. USB serial console (CDC)

Expose a virtual serial port over USB so the shell is reachable over the cable
rather than a UART jig.

**Status:** Not implemented, scaffolding exists. The UF2 bootloader remains
file-transfer-only, and the opt-in `usb-cdc` feature stops with
`UsbError::Unsupported`. The real access paths today are a UART jig and an SWD
probe, see `docs/USB_CDC.md` and `docs/HARDWARE_ACCESS.md`.

### 4. Configurable boot / OTA deployment tools

Polish and harden the deployment story around the UF2 bootloader:

- Let the companion app orchestrate a multi-step flash, verify, and reboot loop.
- Detect a stuck or failed flash and fall back cleanly to a safe state.

**Status:** Partially done. The Rust `sensor-watch-tools` binary provides
host-side validation, known-good backup preservation, manifest output, and
explicit rollback staging. Studio currently refuses configured UF2 builds until
preset, board, and component selections are wired into firmware inputs. USB-device
detection, reboot verification, automatic fail-safe loops, and device-side
rollback remain unimplemented.

### 5. Full real-face host migration

Move the remaining 3 firmware faces into the Studio host seam so all 111
registered firmware faces can run the real firmware implementation in the
simulator.

**Status:** Open. 108 faces currently use the default-enabled `real-faces`
seam; the remaining 3 faces use the `face_sim` engine because their modules are
not yet exported through the host firmware library.

### 6. Physical hardware validation

Execute the procedures in `docs/TESTING.md` on real Sensor-Watch silicon,
including flashing, power, RTC, storage, peripherals, fault recovery, and face
behavior.

**Status:** Open. No on-silicon validation has been performed.
