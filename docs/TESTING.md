# Hardware Test Plan

This document defines the formal test procedure for validating the firmware on
real Sensor Watch hardware. The firmware is host-tested (unit tests, fuzz tests)
but has not yet been validated on silicon. This plan closes that gap.

> **Status:** Not yet executed. This is the procedure to follow once a board is
> available for flashing.

## Prerequisites

- A Sensor Watch board (SAM L22J18A) in bootloader mode (USB mounted drive).
- A multimeter with microamp resolution (for power measurements).
- A stopwatch or the Studio app's NTP time (for RTC accuracy).
- The `.uf2` produced by `./build.sh` or CI.

## 1. Flashing & Boot

| Test | Procedure | Pass criteria |
|------|-----------|---------------|
| Flash | Copy `sensor-watch.uf2` to the bootloader drive | Watch reboots into the firmware; display shows the clock |
| Boot integrity | Power-cycle the watch 10 times | No corruption LED flash; boots to clock each time |
| Boot throttle | Rapidly reset 5+ times within 5 s | Watch enters safe state (dim LED, low-battery symbol) |

## 2. RTC Accuracy

| Test | Procedure | Pass criteria |
|------|-----------|---------------|
| Timekeeping | Set the time via NTP, leave for 24 h, compare | Error < 5 s/day (or record actual PPM for drift correction) |
| Drift | Use the Studio drift tool over 24 h | PPM value reported; correction applied if > 0.5 ppm |
| Alarm wake | Set an alarm, let the watch sleep, wait for alarm | Watch wakes and sounds the alarm at the right time |
| Calendar | Advance through month/year boundaries | Date rolls over correctly (leap years, month lengths) |

## 3. Power Consumption

| Test | Procedure | Pass criteria |
|------|-----------|---------------|
| Standby | Measure current on the main face, seconds hidden | < 10 µA |
| Active | Measure current while a button is held (LED on) | Matches the LED draw (~10 mA) |
| Low-energy | Enable LE mode, wait for timeout | Current drops to standby levels |
| Deep sleep | Enter BACKUP mode | Current < 2 µA; RTC keeps time |
| Battery | Measure at 3.0 V, 2.6 V, 2.2 V | BOD33 triggers at ~2.6 V; low-battery indicator at 2.2 V |

## 4. Flash Wear & Persistence

| Test | Procedure | Pass criteria |
|------|-----------|---------------|
| Settings persist | Change a setting, power-cycle | Setting survives |
| Wear leveling | Write a setting 1000 times | No corruption; wear-leveled rows rotate |
| ECC | Corrupt a stored value (via debug), read it back | ECC corrects single-bit errors |
| Crash recovery | Reset mid-write (via debug) | Wear-leveled read finds the last valid entry |

## 5. Watch Faces

| Test | Procedure | Pass criteria |
|------|-----------|---------------|
| All faces | Cycle through every face in the preset | Each renders correctly and responds to buttons |
| Stopwatch | Start, run 60 s, stop | Shows 1:00 |
| Timer | Set 5 s, start | Counts down and alarms at 0 |
| Alarm | Set, enable, wait | Sounds at the set time |
| Diagnostics | Navigate all submenus | Each screen renders; tests run without crashing |
| Fuzz | Run the Studio fuzz tool on each face | No panics, no invalid display |

## 6. Fault & Recovery

| Test | Procedure | Pass criteria |
|------|-----------|---------------|
| Watchdog | Trigger a hang (via debug), wait | Watchdog resets; `WatchdogReset` fault recorded |
| Panic | Trigger a panic (via debug) | LED blinks, device resets, `Panic` fault recorded |
| Clock failure | Disconnect the 32 kHz crystal | CFD switches to internal oscillator; `ClockFailure` fault |
| Brown-out | Drop VDD below 2.6 V | BOD33 interrupt; safe shutdown |

## 7. Peripherals

| Test | Procedure | Pass criteria |
|------|-----------|---------------|
| LED | Cycle red/green/off | Correct colors, correct polarity per board |
| Buzzer | Play a tone | Audible at the configured voltage |
| Accelerometer (if fitted) | Tap the watch | SingleTap/DoubleTap events fire; raise-to-wake shows seconds |
| I2C | Read the accelerometer | No bus hangs; pins float in standby |

## Reporting

For each test, record: date, board revision, firmware commit, result (pass/fail),
and any measured values (current, PPM, error). Publish the results (e.g. as a
`TESTING.md` update or a release note) so the project's hardware-validation
status is transparent and verifiable.

> **Status:** Not yet executed. This is the procedure to follow once a board is
> available for flashing.
