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

**Status:** Backburner. This is the end-goal app. The firmware itself must be
solid first.

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

### The viable version

The program could generate a **blank file** (or a tiny marker) that, when the
watch detects it, resets the clock to the current minute. The watch reads it on
the next minute boundary and sets the time precisely.

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
toggle (bottom-right button) works on all boards.

---

## 5. BACKUP-Mode "Power Off" Feature

A menu option that puts the watch into its deepest sleep (BACKUP mode) when not
worn. Before entering BACKUP, save state to flash; on wake, restore it.

**Status:** Backburner. STANDBY is the primary mode (retains RAM + display).
BACKUP would be an optional extreme-power-save feature.
