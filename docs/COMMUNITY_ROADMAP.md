# Community Feature and Integration Roadmap

This document records the complete community feature inventory collected from
the private `sensor-watch-dev` discussion export and upstream reference work.
It is broader than `docs/BACKBURNER.md`: completed work is listed for
traceability, while only genuinely unfinished items belong in the Backburner.

## Status key

- **Implemented in Rust** - present in the current firmware, core, Studio, or
  tools code. Physical validation may still be pending.
- **Implementable now** - can be advanced with the existing hardware model and
  host/firmware architecture.
- **Hardware-dependent** - requires a board, sensor, electrical modification,
  custom LCD, jig, or on-silicon measurement.
- **Speculative** - useful research or product ideas without a defined,
  validated implementation path.

## Implemented or substantially represented in Rust

- SAM L22 firmware HAL: RTC, SLCD, GPIO/EIC, LED, buzzer, ADC, I2C, SPI, UART,
  storage, deep sleep, watchdog, CRC/ECC, memory, LIS2DW, and shell.
- Event-driven Movement framework with RTC-owned timekeeping, scheduled tasks,
  debounce, long-press handling, fault tracking, buzzer priority, and primary /
  secondary face registration.
111 registered firmware faces, with 93 faces currently running through the
  real Studio host seam and 18 using the simulator fallback.
- Alarm, advanced alarm, countdown, timer, stopwatch, quick utility, metronome,
  repetition-minute, time-left, deadline, and world-clock families.
- DST-aware timezones, UTC/local-time conversion, NTP, drift correction,
  frequency correction, nanosecond calibration, sunrise/sunset, astronomy,
  planetary time/hours, solar time, moon phase, and location-aware faces.
- TOTP/TOTP-LFS, calculators, RPN calculators, MorseCalc, Wordle, games,
  probability, tarot, geomancy, databank, habit, hydration, activity, and
  sensor/temperature/voltage faces.
- LIS2DW accelerometer driver, tap detection, motion wake, accelerometer data
  acquisition, sensor logging, and host tests.
- UF2 encoding/validation, host recovery staging, manifests, CRC/ECC, fault
  records, panic fingerprints, SWD/probe scaffolding, and CI.
- Firmware Studio editor, Blocks mode, simulator, real-face seam, presets,
  settings, NTP, calibration, shell, diagnostics, modules, wiki/tutorials,
  bug reports, source export, and i18n.
- Host-testable pure logic and mock-backed firmware face tests.

## Implementable software parity work

### Framework and interaction

- Standardize button roles and publish UI guidelines for every face.
- Add a typed modal-face/call-face API for shared settings and utilities.
- Centralize face resignation, timeout, and advisory policy.
- Add explicit tests for 1/4/16/64/128 Hz tick requests, button events during
  buzzer playback, wake-and-hold Alarm behavior, and simultaneous inputs.
- Add a reusable settings/modal framework for faces.
- Add a Casio-style quick preset timer integrated with the clock workflow.
- Add multiple concurrent timer slots where the hardware/event model permits it.
- Add quiet hours and separate button/signal/alarm volume policies.
- Add runtime event-specific tunes and optional RTTTL parsing.
- Add a face build identifier and firmware provenance display.

### Faces and algorithms

- Finish the remaining 20 real-face host migrations with face-specific harnesses
  rather than generic activation assumptions.
- Add a conservative LIS2DW step-counter algorithm using replayable recorded
  samples, bounded FIFO reads, and a power budget.
- Add a 113-city location preset selector.
- Add prayer-times calculations as a separate optional face/module.
- Add a true audio/frequency tuner only if an input source is available; the
  standard watch piezo is not a safe microphone input.
- Add quick timer, activity export, battery-voltage history, and improved
  diagnostics where they fit the current storage budget.
- Add reusable display/glyph mapping tests for ambiguous LCD characters.
- Add partial-display-update benchmarking and state-diff rendering where it
  produces measurable power savings.

### Tooling and Studio

- Wire presets, selected faces, board profiles, and component profiles into the
  actual firmware build inputs. Studio currently refuses these builds rather
  than publishing an unconfigured artifact.
- Generate board-specific compile-time configuration for pins, LCD, sensors,
  LED polarity, buzzer voltage, and optional modules.
- Add minimal firmware profiles as well as the full 111-face regression build.
- Resolve Cargo, objcopy, probe-rs, and firmware workspace paths from verified
  installation/configuration roots rather than ambient PATH/current directory.
- Add a stale-build-lock recovery workflow with PID/timestamp ownership.
- Add deterministic CLI workspace resolution independent of the caller's cwd.
- Add simulator tests for low-energy wake, RTC compare callbacks, buzzer timing,
  browser refresh state, and face/preset transitions.
- Add a beginner-safe save workflow in Blocks mode with visible identity and
  save/load controls.
- Add explicit `PASS (SIMULATED)`, `NOT AVAILABLE`, and `NOT TESTED` diagnostics
  terminology.
- Add trusted release signatures rather than calling a self-hash a signature.

### Protocols and recovery

- Complete host-side optical/IrDA framing tests for loss, duplication,
  truncation, retry, resume, version, target, and chunk validation.
- Define a resumable patch/full-image protocol before hardware integration.
- Add signed firmware manifests with an embedded public key or clearly label all
  current hashes as local integrity checks only.
- Add authenticated NTP/NTS or an authenticated HTTPS time source.
- Add a transport abstraction shared by UART now and native USB CDC later.
- Add physical-presence/session authentication for mutating shell commands.
- Add host simulators for optical send/receive and transfer recovery.

## Hardware-dependent work

- Execute the full `docs/TESTING.md` plan on real Sensor-Watch silicon.
- Validate standby/deep sleep current and wake latency.
- Validate VBUS/battery behavior at low voltage and during USB detection.
- Validate LCD segments, display bias, partial refresh, and custom LCD routing.
- Validate buzzer waveform, boosted voltage, sequence timing, and acoustic transfer.
- Validate LIS2DW thresholds, FIFO servicing, I2C timeouts, motion wake, and step
  counting on a fitted accelerometer.
- Add LIS2DUX12/BMA421 integrated step-counter modules if a supported board is
  available.
- Add pressure/altitude hardware and a dive-computer face only behind an
  explicit external-sensor module.
- Integrate optical/IrDA receiver/transmitter hardware and measure transfer
  reliability and current draw.
- Integrate NFC flashing/charging only as a separate custom-board project.
- Integrate BLE only if the project intentionally changes its air-gapped design.
- Add GPS-assisted calibration only with a defined external fixture.
- Validate SWD, UART jig, external flash, custom LCD, and development-board
  workflows physically.

## Speculative research

- QEMU or Microchip full-chip emulation.
- DMA/ABM autonomous LCD animations.
- MicroPython, Forth, Lisp, Rust-face, or Zig-face runtime environments.
- Audio modem/RTTY/ggwave firmware transfer beyond the Chirpy/Fesk experiments.
- Screen-flicker optical updates for tiny calendar/TOTP payloads.
- NFC/BLE health synchronization and Google Health export.
- Solar charging, supercapacitors, alternate batteries, or wireless power.
- Custom Data Bank/Timex-style LCDs and alternate Casio cases.
- Piezo-as-microphone, pulse detection from accelerometer, and advanced health
  estimation without dedicated sensors.

## Community bug regression suite

The following historical failures should remain explicit test cases where the
Rust architecture can model them:

- Lost button-up events and stuck LEDs after debounce/long-press sequences.
- Lost background-task or RTC compare events at high tick frequencies.
- Timer/countdown expiry at exact wake boundaries.
- Leap-day, month rollover, timezone, and DST transition behavior.
- Simulator UTC/local-time double conversion.
- Low-energy wake and first-frame redraw.
- TOTP immediate redraw after button input.
- Buzzer note/rest duration and priority collisions.
- I2C timeout and accelerometer FIFO overflow/freeze.
- Storage power loss after erase, header write, and payload write.
- VBUS detection and low-voltage behavior.
- UF2 corruption, wrong board, wrong target, and interrupted transfer.
- Face deletion, preset persistence, source registration, and simulator fallback.

## What is intentionally not claimed

- The Rust host test suite is not physical validation.
- UF2 structural validation is not firmware authenticity.
- A UART jig is not native USB CDC.
- Protocol-only optical framing is not optical hardware integration.
- A simulated sensor/diagnostic PASS is not a measured hardware PASS.
- A configured Studio preset is not part of a firmware artifact until the build
  inputs are actually wired into the embedded build.
