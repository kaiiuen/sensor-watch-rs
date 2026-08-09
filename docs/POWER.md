# Sensor-Watch Firmware - Power Management Deep-Dive

This document explains the power-management architecture in detail: the power
states, the event-driven model, the resource budget, and the "totalitarian"
control mechanisms that keep power usage minimal.

---

## The core principle

> **The CPU is a start/stop resource. It wakes only to react to a single event,
> then immediately returns to STANDBY. All timekeeping is owned by the RTC,
> never by the CPU.**

This means:
- The CPU is **never idle-running**. It's either asleep or doing one reaction.
- The watch **never polls**. It reacts to interrupts.
- **Nothing is left on** after a reaction.

---

## Power states

The SAM L22 has these states. This firmware uses **STANDBY** as its primary mode.

| State | CPU | Main clock | RAM | RTC | Peripherals | Wake |
|-------|-----|-----------|-----|-----|-------------|------|
| **ACTIVE** | ✅ | ✅ 4 MHz | ✅ | ✅ | ✅ all | - |
| **IDLE** | ⏸ | ✅ | ✅ | ✅ | ✅ all | any interrupt |
| **STANDBY** | ⏸ | ❌ off | ✅ retained | ✅ | ⚠️ selective | any interrupt |
| **BACKUP** | ❌ | ❌ off | ❌ **lost** | ✅ | ❌ none | RTC alarm, A2/A4 |

### Why STANDBY and not BACKUP?

- **STANDBY** retains RAM and the LCD, wakes in µs. Correct for a running watch.
- **BACKUP** powers off RAM and CPU. It's a "power off" state, not a running
  state. It can't display the time or run faces.

The firmware uses STANDBY. The CPU sleeps, peripherals auto-release.

---

## The event-driven model

The main loop is a thin dispatcher:

```
Interrupt fires
  └─► sets PENDING_EVENT
  └─► CPU wakes
  └─► app_loop() reacts to ONE event
  └─► release_peripherals()
  └─► kick watchdog
  └─► enter_standby()  (SysTick-safe STANDBY)
```

The CPU is active for **microseconds to a couple milliseconds** per event -
just long enough to update the display or handle a button. Then it sleeps.

### Why SysTick-safe standby?

If the SysTick interrupt fires at the exact microsecond the CPU enters STANDBY
while back-bias is enabled, the SAM L22 throws a Hard Fault. `enter_standby()`
disables the SysTick interrupt immediately before `wfi()` and re-enables it
right after waking, eliminating that race.

### The closed event set

The CPU wakes only for one of these:

- `Activate` - a face entered the foreground
- `Tick` - the RTC ticked
- `BackgroundTask` - a scheduled task is due
- `Button(button, event)` - a button was pressed
- `SingleTap` / `DoubleTap` - the accelerometer detected a tap
- `AccelerometerWake` - the accelerometer detected motion

A **closed enum** means every event is known and handled. No ambiguity.

---

## Power-management mechanisms

### 1. Dynamic tick rate

The single biggest power saver. When **seconds are hidden**, the watch wakes
**once per minute** instead of once per second.

- Seconds shown → 1 Hz tick (wake 86,400×/day)
- Seconds hidden → 1/minute (wake 1,440×/day)

This is controlled by the `show_seconds` setting and `set_tick_rate()`.

> **Adaptive power:** the 128 Hz fast tick is now power-adaptive. It runs at
> 128 Hz only while the user is interacting (for long-press + debounce), then
> suspends after ~10 s of inactivity so the CPU sleeps at the base 1 Hz / minute
> rate. The watchdog timeout extends to cover the longer idle sleep and tightens
> again on button activity, so it backs off when nothing is happening instead of
> waking constantly. Note the watchdog period change and battery current should
> still be validated on silicon.

### 2. Peripheral auto-release

After every `app_loop`, `release_peripherals()` disables ADC, I2C, and SPI, and
reconfigures the I2C pins to floating inputs. The LCD, RTC, and buttons stay on
(they're needed). This ensures no face can leave a peripheral on to drain the
battery, and no sensor board can backward-power itself through the I2C bus.

### 3. Zero-heap

There is no allocator. Everything is a `static` instance or a fixed-size stack
buffer. Memory growth is structurally impossible. No allocation overhead, no
leaks.

### 4. Watchdog

The WDT resets the chip if the main loop hangs. This guarantees recovery and
prevents a hang from causing a long active time (which would drain the battery).
It is only refreshed from the main loop (windowed clearing), so a runaway
interrupt loop cannot mask a hang.

### 5. Bounded waits

Every hardware polling loop uses `wait_until()` with a timeout. A hung
peripheral returns an error instead of hanging the CPU.

### 6. RAM for state, flash only for settings

- **RAM** (always-on in STANDBY) holds runtime state. No flash wear.
- **Flash** holds only persistent settings, written rarely and wear-leveled.

### 7. Brown-out protection

BOD33 monitors VDD and interrupts before the rail falls below the CPU threshold.
The boot-count throttle detects a brown-out reboot loop and drops the watch into
a safe state (buzzer/LED disabled) until the battery is replaced.

---

## Power estimates

| State | Current | Runtime (90 mAh) |
|-------|---------|------------------|
| Seconds hidden (LE) | ~6.5 µA | ~1.6 years |
| Normal (seconds on) | ~10 µA | ~1.05 years |
| High active | ~30 µA | ~3-4 months |

### The math

Energy = power × time. The load (current) is roughly fixed per operation, so the
only lever is **active time per event**.

- At **1 ms per wake**, 86,400 daily ticks cost ~0.17 mAh/day → ~1.2 years.
- At **500 ms per wake**, 86,400 daily ticks cost ~86 mAh/day → ~1 week.

So the goal is: **keep every wake under ~1 ms.** The event-driven model does
this - a reaction is a handful of register writes.

---

## Resource budget

### Flash (256 KB)

- Firmware region: `0x3A000` (~232 KB) after the bootloader
- Release binary with all 111 faces: roughly 210-230 KB
- Each face: ~1-3 KB
- Headroom: modest (a few more faces), not dozens

### RAM (32 KB)

- Static state: ~300 bytes
- Stack: placed at the bottom of RAM via flip-link (a stack overflow triggers
  an immediate hard fault rather than silent corruption); no fixed 8 KB carve-out
- Heap: none

RAM is not a constraint. All face state lives in RAM (state survives face
switches, no flash wear).

---

## The "totalitarian" control summary

Every mechanism is designed so nothing goes rogue:

| Concern | Control |
|---------|---------|
| CPU stays awake | Event-driven dispatcher, always sleeps after a reaction |
| Peripheral left on | Auto-release after each reaction |
| Sensor board leaks via I2C | I2C pins floated before sleep |
| Memory grows | Zero-heap, static-only |
| Software hangs | Watchdog resets the chip |
| Interrupt loop masks a hang | Windowed watchdog clearing |
| Peripheral hangs | Bounded waits return an error |
| Flash wears out | Wear-leveled, write-verified storage |
| Flash bit-rot | CRC-32 integrity check |
| Settings lost | Persisted to flash, loaded on boot |
| Silent failure | Fault system with LED codes |
| Brown-out reboot loop | BOD33 + boot-count throttle |
| Battery drain | Dynamic tick rate (hide seconds → wake 1/min) |
