# Sensor-Watch Firmware (Rust) - Architecture & Reference

This document is the authoritative reference for the Sensor-Watch firmware rewrite
in Rust. It explains **every module, every symbol, and the design rationale** behind
each choice. It is written to be complete enough that a new contributor can
understand the entire system without reading the source line-by-line.

---

## 1. Overview

The Sensor Watch is a board replacement for the classic Casio F-91W wristwatch.
It is built around the **Microchip SAM L22J18A**, an ARM Cortex-M0+ microcontroller
with:

- **256 KB flash** (firmware storage)
- **32 KB RAM** (runtime data)
- A **segment LCD** (10 digits + 5 indicators)
- **3 buttons** (Light, Mode, Alarm)
- A **bi-color LED** (red/green, or red/blue on some boards)
- A **piezo buzzer**
- A **real-time clock (RTC)** with a 32 kHz crystal
- A **9-pin connector** for optional I2C/SPI sensor boards

This project is a **from-scratch rewrite** of the original C firmware in Rust. It
is structured as a **Cargo workspace** with two crates:

```
sensor-watch-rs/
|-- Cargo.toml          # workspace manifest + firmware package
|-- core/               # sensor-watch-core: pure logic, host-testable
|   `-- src/
|       |-- lib.rs      # crate root
|       |-- datetime.rs # packed date/time type
|       |-- settings.rs # settings bit-packing
|       |-- utility.rs  # date/time math
|       `-- uf2.rs      # UF2 encoding (used by the build tool)
`-- src/                # the firmware binary
    |-- main.rs         # reset handler / entry point
    |-- panic.rs        # panic handler (self-recovery)
    |-- watch/          # hardware abstraction layer (HAL)
    `-- movement/       # watchface framework + faces
```

### Why two crates?

The `core` crate contains **pure computation** (date math, bit-packing, UF2
encoding) with **no hardware dependency**. This lets us run **unit tests on the
host** (a normal desktop), giving us *proof* the logic is correct without needing
the physical watch. The firmware crate (`sensor-watch`) depends on the PAC and can
only build for the embedded target.

This separation also enforces the project rule of keeping **dependencies separate
from the application**: the core logic is a library, the firmware is the app.

### Reference repos

The original C sources are kept in sibling directories purely as references:

- `sensor-watch-reference/` - the original Sensor-Watch C repo
- `second-movement-reference/` - the Second Movement C repo

The Rust rewrite is being merged with Second Movement's features (persistent
settings, DST-aware timezones, buzzer priorities, accelerometer support, and many
new faces). We do not modify the reference repos.

---

## 2. The core design philosophy

The entire firmware is built around one central idea:

> **The CPU is a start/stop resource. It wakes only to react to a single event,
> then immediately returns to STANDBY. All timekeeping is owned by the RTC,
> never by the CPU.**

This has several consequences:

1. **No polling.** The CPU never spins waiting for something. It either has the
   data (from an interrupt) or schedules a future wakeup via the RTC alarm.
2. **No idle CPU time.** Between events, the CPU is in STANDBY, drawing
   microamps instead of milliamps.
3. **Zero-heap.** There is no allocator. Everything is a `static` instance or a
   fixed-size stack buffer. Memory growth is *structurally impossible*.
4. **Deterministic.** Every event maps to exactly one handler. There are no
   ambiguous states.
5. **Self-managing.** The watch runs sealed and air-gapped. It must recover from
   faults, detect corruption, and manage its own power with no external help.

This is a deliberate departure from the original C "Movement" framework, which
used a polling loop. The event-driven model is more power-efficient and more
robust for a sealed, air-gapped wearable.

---

## 3. Power states and power management

The SAM L22 has several power states. This firmware primarily uses **STANDBY**.

| State | CPU | Main clock | RAM | RTC | Peripherals | Wake |
|-------|-----|-----------|-----|-----|-------------|------|
| **ACTIVE** | yes | yes, 4 MHz | yes | yes | all | - |
| **IDLE** | paused | yes, 4 MHz | yes | yes | all | any interrupt |
| **STANDBY** | paused | off | retained | yes | selective | any interrupt |
| **BACKUP** | no | off | **lost** | yes | none | RTC alarm, A2/A4 |

### Why STANDBY and not BACKUP?

- **STANDBY** retains RAM and the LCD display, and wakes in microseconds. This
  is the correct mode for a *running* watch - it keeps state and the display.
- **BACKUP** powers off RAM and the CPU. It is a "power off" state, not a
  running state. It can't display the time or run faces.

The firmware uses STANDBY as its primary mode. The CPU sleeps, and peripherals
are **auto-released** after each reaction so nothing is left on to drain the
battery.

### The power-management lock-down

Several mechanisms work together to minimize power:

1. **Event-driven dispatcher** - the CPU only runs to react, then sleeps.
2. **Peripheral auto-release** - after each `app_loop`, ADC/I2C/SPI are disabled
   and the I2C pins are reconfigured to floating inputs.
3. **Dynamic tick rate** - when seconds are hidden, the watch wakes once per
   minute instead of once per second (the single biggest power saver).
4. **Zero-heap** - no allocator overhead, no memory growth.
5. **Watchdog** - guarantees recovery from any hang.
6. **SysTick-safe standby** - SysTick is disabled around `wfi()` to avoid the
   standby-entry hard fault.

### Power estimates

| State | Current | Runtime (90 mAh) |
|-------|---------|------------------|
| Seconds hidden (LE) | ~6.5 uA | ~1.6 years |
| Normal (seconds on) | ~10 uA | ~1.05 years |
| High active | ~30 uA | ~3-4 months |

The key lever is **active time per event**, not load. At ~1 ms per wake, the
86,400 daily tick wakes barely dent the battery.

---

## 4. Resource budget

### Flash (256 KB)

The firmware region is `0x3A000` (~232 KB) starting after the bootloader.
With all 111 faces registered (via the `#[used]` face-retain array), a release
build is roughly **210-230 KB**, so most of the region is used. Each watch face
adds roughly 1-3 KB, leaving modest headroom for a few more faces, not dozens.
Adding faces beyond the current count should be validated against the linker
region on every build. Studio currently wires 108 of those faces into its
default-enabled `real-faces` host seam, the remaining 3 faces use the
`face_sim` fallback engine in Studio.

### RAM (32 KB)

- **Static state** (all faces + movement): a few hundred bytes
- **Stack**: reserved in the linker script
- **Heap**: none (zero-heap)

RAM is essentially not a constraint. All face state lives in RAM, which means
**state survives face switches** and **no flash wear from runtime state**.

### Why RAM for state, flash only for settings?

- **RAM** is volatile but always-on in STANDBY. It holds runtime state (face
  state, counters) - fast, no wear.
- **Flash** is non-volatile. It holds *persistent* settings (board config,
  timezone) that must survive a full power-off. It's written only on explicit
  save, and wear-leveled.

---

## 5. Boot sequence

The reset handler in `src/main.rs` runs in this exact order:

```
1. copy_ramfunc()                        - copy .ramfunc routines from flash to RAM
2. movement::fault::check_reset_reason() - record why we reset (watchdog/panic/power-on)
3. movement::fault::check_boot_throttle()- detect a brown-out reboot loop
4. wdt::init()                         - initialize the watchdog before
                                            boot-time clock waits
5. watch::crc::check_firmware_integrity() - CRC-32 integrity check, on failure
                                            record a CorruptImage fault and keep
                                            booting best-effort (non-bricking)
6. watch::init()                         - hardware init in dependency order:
     a. irq::init()                      - interrupt priorities
     b. clock::init()                    - 32 kHz crystal + GCLK routing,
                                            with bounded oscillator wait
     c. rtc::init()                      - RTC (depends on the clock)
     d. wdt::init()                      - idempotent watchdog backstop
7. movement::fault::check_clock_failure()- detect a failed 32 kHz crystal (CFD)
8. watch::deepsleep::init_bod33()        - brown-out detector (low-battery interrupt)
9. movement::app_init()                  - load settings, init framework
10. movement::board::apply()              - apply LED polarity + buzzer voltage
11. movement::app_setup()                - register faces, buttons, alarms
12. watch::rtc::register_tick_callback() - 1 Hz tick
13. loop:
     a. movement::app_loop()             - react to one event
     b. watch::wdt::kick()               - watchdog heartbeat
     c. watch::deepsleep::enter_standby()- enter STANDBY (SysTick-safe)
```

The order matters: each step depends on the previous. Interrupt priorities must
be set before any interrupt is enabled, the RTC needs the clock, the watchdog
needs to be started after the system is ready.

---

## 6. The `watch` module (Hardware Abstraction Layer)

The `watch` module wraps every SAM L22 peripheral behind a safe Rust API. Each
submodule corresponds to one peripheral or subsystem.

### 6.1 `watch/mod.rs` - module root

Declares all submodules and provides `watch::init()`, which enforces the
dependency-ordered initialization.

### 6.2 `watch/irq.rs` - interrupt priorities

Sets NVIC priorities so a critical interrupt can never be blocked by a less
critical one. On ARMv6-M, a **lower** value = **higher** urgency.

| Interrupt | Priority |
|-----------|----------|
| RTC alarm | 0 (highest) |
| RTC tick | 1 |
| Buttons (EIC) | 2 |
| TC3 (buzzer) | 3 |

**Why:** the RTC alarm must always fire to wake the watch. If a button interrupt
could preempt it indefinitely, the watch could miss a wakeup.

### 6.3 `watch/clock.rs` - clock initialization

Enables the **32 kHz external crystal (XOSC32K)** and routes its 1 kHz output to
the RTC. Also enables the RTC's APB clock.

**Why:** the RTC needs a clock source before it can run. This is the foundation
everything else depends on.

### 6.4 `watch/rtc.rs` - Real-Time Clock

The heart of the watch. Provides:

- `DateTime` - a packed date/time type (6-bit second, 6-bit minute, 5-bit hour,
  5-bit day, 4-bit month, 6-bit year since 2020)
- `init()` - configures the RTC in clock/calendar (MODE2) mode
- `set_date_time()` / `get_date_time()` - read/write the clock (with `SYNCBUSY`
  enforcement)
- `register_tick_callback()` - 1 Hz tick
- `register_periodic_callback()` - configurable tick (1-128 Hz, power of 2)
- `register_alarm_callback()` - alarm at a specific time
- `schedule_wakeup()` / `schedule_wakeup_in()` - one-shot future wakeups
- `enable()` / `freqcorr_write()` - RTC control and crystal drift correction
- The `RTC()` interrupt handler dispatching tick/tamper/alarm callbacks

**Why the RTC owns all timekeeping:** the RTC keeps counting in STANDBY. So the
CPU can sleep and the RTC tracks elapsed time. This is what makes the
event-driven model power-efficient.

### 6.5 `watch/slcd.rs` - Segment LCD

Drives the 10-digit segment display. Provides:

- `enable_display()` - init + enable the SLCD peripheral
- `set_pixel()` / `clear_pixel()` - direct segment control
- `display_character()` / `display_string()` - render text
- `set_colon()` / `clear_colon()`
- `set_indicator()` / `clear_indicator()` - the 5 indicator segments
- `start_character_blink()` / `start_tick_animation()` - autonomous effects

The `CHARACTER_SET` and `SEGMENT_MAP` tables map ASCII characters to the 7-segment
bit patterns and the physical (common, segment) pins.

**Why:** the LCD retains its display in STANDBY with no CPU involvement, so the
time stays visible while the CPU sleeps.

### 6.6 `watch/gpio.rs` - GPIO

Provides pin direction, pull, function, and level control. The `Pin` type is
`(port, pin)`. Supports arbitrary PMUX function values (needed for peripherals
like TCC/ADC).

### 6.7 `watch/extint.rs` - Buttons / External Interrupts

Configures the EIC to detect button presses. Provides:

- `enable_external_interrupts()` / `disable_external_interrupts()`
- `register_interrupt_callback()` - set up a button interrupt
- The `EIC()` interrupt handler

The buttons are active-high, pulled down internally.

### 6.8 `watch/led.rs` - Bi-color LED

Drives the LED via TCC0 in PWM mode. Provides `enable_leds()`, `set_led_color()`,
`set_led_red()`, `set_led_green()`, `set_led_off()`, `set_led_color_rgb()`, and
`set_invert_polarity()` (for common-anode Red/Pro boards).

**Why PWM:** PWM lets us set arbitrary brightness/color. The TCC outputs hold
their state in STANDBY, so the LED stays lit while the CPU sleeps.

### 6.9 `watch/buzzer.rs` - Piezo Buzzer

Drives the buzzer via TCC0 (shared with the LED) plus a TC3 timer for non-blocking
note sequences. Provides `enable_buzzer()`, `set_buzzer_period()`, `set_buzzer_on()`,
`set_buzzer_off()`, `play_note()`, `play_sequence()`, and the `Note` enum (A1-B8 +
Rest).

### 6.10 `watch/adc.rs` - Analog-to-Digital Converter

Reads the battery voltage and the five analog-capable pins. Provides
`enable_adc()`, `get_analog_pin_level()`, `get_vcc_voltage()`, and reference
voltage selection.

**Why:** the battery rail is measured against the internal reference. Because the
PCB lacks an isolated reference, the raw value *rises* as the battery weakens, so
`get_vcc_voltage()` applies the inverse scaling `V = 1.0 V * ADC_Max / ADC_Raw`.

### 6.11 `watch/i2c.rs` - I2C

Uses SERCOM1 in I2C master mode. Provides `enable_i2c()`, `send()`, `receive()`,
register helpers, and `pins_to_floating_before_sleep()`.

**Why `pins_to_floating_before_sleep()`:** a sensor board on the 9-pin connector
is powered from the same LDO rail as the SAM L22, so it can backward-power itself
through the SDA/SCL pull-ups while the CPU sleeps. Reconfiguring the pins to
floating inputs halts that leakage.

### 6.12 `watch/spi.rs` - SPI

Uses SERCOM3 in SPI master mode. Provides `enable_spi()`, `write()`, `read()`,
`transfer()`.

### 6.13 `watch/uart.rs` - UART

Uses SERCOM3 in USART mode. Provides `enable_uart()`, `puts()`, `getc()`.

### 6.14 `watch/storage.rs` - Flash storage

Provides read/write/erase access to the 8 KB RWW EEPROM emulation area. Includes:

- `read()` / `write()` / `erase()` - raw access, `write()` and `erase()` run from
  RAM (`.ramfunc`) to avoid the read-while-write bus stall
- **Write-verify** - reads back after write to confirm success
- **Wear leveling** - `wear_leveled_write()` rotates writes across 8 rows so no
  single row wears out first

**Why `.ramfunc`:** if the CPU runs a function in flash bank A while writing to
bank B (the RWW EEPROM area), the memory bus stalls. Running the write routines
from RAM keeps the CPU active during the write.

**Why wear leveling:** flash has limited write cycles (~100k). Rotating writes
extends the life of the EEPROM area.

### 6.15 `watch/deepsleep.rs` - Sleep control

Provides external wake callbacks, backup data storage, and the sleep modes.
Includes `enter_standby()` (the SysTick-safe main-loop sleep) and `init_bod33()`
(the brown-out detector configuration).

### 6.16 `watch/timeout.rs` - Bounded waits

Provides `wait_until()` (with a timeout) and the `Error` type. Every hardware
polling loop is bounded so a hung peripheral can never hang the CPU.

**Why:** a `while !flag {}` with no bound means a hung bus = hung watch. With a
timeout, the driver returns an error instead.

### 6.17 `watch/wdt.rs` - Watchdog Timer

The hardware backstop. If the main loop stops kicking it (because of a hang),
it resets the whole chip. `init()` sets a ~2 second timeout with always-on
behavior (can't be disabled by software). `kick()` reloads it. `kick_windowed()`
documents that the watchdog is only refreshed from the main loop, so a runaway
interrupt loop cannot mask a hang.

**Why:** the panic handler only catches panics, not hangs. The watchdog catches
hangs, guaranteeing the watch always recovers.

### 6.18 `watch/crc.rs` - CRC-32 integrity check

Computes a CRC-32 over the firmware text region to detect flash bit-rot. A
mismatch is recorded as the distinct `Fault::CorruptImage` diagnostic and the
firmware continues best-effort, this is not a recovery image, rollback selector,
or true dual-boot implementation. Replacement requires a validated host UF2
artifact and the existing ROM/UF2 bootloader path.

### 6.19 `watch/ecc.rs` - SECDED error correction

Encodes each 32-bit data word with a 7-bit Hamming SECDED code (39 bits).
`decode()` corrects any single-bit error and detects any double-bit error, so
flash bit-rot is corrected on read rather than silently corrupting data.

### 6.20 `watch/shell.rs` - Serial command shell

A minimal command interpreter over the debug UART. Provides `time`,
`settime YYMMDDHHMMSS`, `drift`/`drift N`, `optical`, `panic`, `events`,
`events clear`, and `help` commands. Optical support is an experimental,
receive-only path for Pro hardware and has not been tested on hardware. RX is
nonblocking with
a bounded ring and bounded line/error responses. Mutating commands are always
fail-closed. With the `shell-auth` feature enabled, they are authorized by
`movement::shell_auth` only while the physical Alarm/service button is held.
Without that feature they remain locked. The 30-second window expires
automatically, and a button release revokes it immediately. Read-only commands
remain available.

### 6.21 `watch/memory.rs` - Memory usage

Reports RAM usage from the linker symbols (`.data` + `.bss`). Used by the
diagnostics face to show the firmware's static RAM footprint.

### 6.22 `watch/utility.rs` - Utility functions

Date/time helpers: weekday, week number, leap year, UNIX time conversion,
durations, 12-hour conversion, thermistor temperature.

### 6.23 `watch/utz.rs` / `watch/zones.rs` - DST-aware timezones

Port of the `utz` micro timezone library. `utz.rs` provides the DST rule
engine (day-of-week math, rule unpacking, offset calculation), `zones.rs`
holds the 46-zone table with their offsets and DST rules. The movement layer
uses these to compute the correct local time across daylight-saving
transitions.

### 6.24 `watch/lis2dw.rs` - LIS2DW accelerometer

Port of the C `lis2dw.c` driver. Talks to the LIS2DW12 accelerometer on the
9-pin connector over I2C. Provides data-rate/mode/range configuration, raw
reading, FIFO access, and tap/wake-on-motion detection. The movement layer
uses this to offer tap detection and accelerometer wake to faces.

### 6.25 `watch/rtc.rs` - compare-callback queue

A software analog of the Second Movement hardware compare-callback queue.
Up to 8 indexed slots (`N_COMP_CB`) each hold a target time and callback, the
earliest pending slot is armed via the existing one-shot alarm. Faces can
schedule indexed timeouts (button, LED, resign, sleep, minute) without
keeping the CPU awake.

### 6.26 `watch/storage.rs` - storage usage

Reports the RWW EEPROM area's total and used size. Used by the diagnostics
face to show storage usage as a percentage.

---

## 7. The `movement` module (Watchface Framework)

The `movement` module is the application layer - it manages watch faces and
dispatches events.

### 7.1 `movement/types.rs` - Core types

- `Settings` - a 32-bit packed struct of user preferences (bit-packing with
  getters/setters), `#[repr(C, align(4))]` for flash alignment. Stored in RTC
  backup register 0.
- `Button` - Light, Mode, Alarm
- `ButtonEvent` - Down, Up, LongPress, LongUp, ReallyLongPress
- `Event` - the closed set of events: Activate, Tick, BackgroundTask, Button,
  SingleTap, DoubleTap, AccelerometerWake
- `WatchFace` - the trait every face implements (with optional
  `wants_background_task()` and `advise()`)
- `MovementState` - global framework state
- `ClockMode` - 12H / 24H / 024H
- `BuzzerPriority` - Button / Signal / Alarm
- `MOVEMENT_SECONDARY_FACE_INDEX` - the start of the secondary face list
- `TIMEZONE_OFFSETS` - the timezone table

**Why a closed `Event` enum:** the CPU wakes only for one of these. A closed enum
means there are no ambiguous or unexpected events - everything is deterministic.

### 7.2 `movement/mod.rs` - The dispatcher

The core of the framework:

- `app_init()` - load settings, init state
- `app_setup()` - register faces, buttons, alarms
- `app_loop()` - react to the single pending event, then return
- `set_tick_rate()` - switch between 1 Hz and 1/minute wake rates
- `move_to_face()` / `move_to_next_face()` - face switching. If a secondary
  face list is configured (`MOVEMENT_SECONDARY_FACE_INDEX`), rotation stays
  within the current list, and Mode long-press jumps from face 0 to the
  secondary list (or back to face 0).
- `default_loop_handler()` - standard button behavior, including the secondary
  list long-press jump and LED auto-off when `led_duration == 0`.
- `schedule_background_task()` - schedule a future wakeup
- `save_settings()` / `store_settings()` - persist settings to flash
- `release_peripherals()` - disable unused peripherals after each reaction
- DST-aware timezones via the `utz` library: `get_current_timezone_offset()` and
  `get_current_timezone_offset_for_zone()` apply daylight-saving rules per
  zone and date.
- Buzzer priority: `play_note()` / `play_sequence()` / `play_alarm_beeps()`
  enforce `BuzzerPriority` (button < signal < alarm) so a lower-priority sound
  cannot interrupt a higher-priority one.
- Accelerometer framework: `accelerometer_begin()`, `enable_tap_detection()`,
  `disable_tap_detection()`, `handle_accelerometer_event()` route `SingleTap` /
  `DoubleTap` / `AccelerometerWake` events to faces.
- Indexed timeouts via the RTC compare queue: `register_timeout(TimeoutIndex,
  DateTime)` / `disable_timeout(TimeoutIndex)`.
- The full Second Movement API surface: `get_local_date_time()`,
  `get_utc_timestamp()`, `set_utc_date_time()`, `set_local_date_time()`,
  `set_timezone_index()`, `get_current_timezone_offset()`, `clock_mode_24h()`,
  `button/signal/alarm_volume()`, `get/set_fast_tick_timeout()`,
  `get/set_low_energy_timeout()`, `alarm_enabled()`, `get/set_backlight_dwell()`,
  `backlight_color()`, `force_led_on/off()`, `request_tick_frequency()`,
  `request_sleep/wake()`, `play_note()`, `play_sequence()`

The interrupt callbacks (`cb_tick`, `cb_light_btn_interrupt`, etc.) set
`PENDING_EVENT`, which `app_loop()` reads and dispatches.

### 7.3 `movement/debounce.rs` - Button debouncing

Mechanical buttons bounce for 5-20 ms. This module requires 4 consecutive stable
samples before accepting a state change, filtering out spurious edges. Also
handles long-press and really-long-press detection via the 128 Hz fast tick.

**Why:** without debouncing, a single button press could register as multiple
presses.

### 7.4 `movement/fault.rs` - Fault/error system

A central "authoritarian watchdog" that tracks system health:

- `Fault` enum - WatchdogReset, Panic, WakeTooLong, InvalidState, BatteryLow,
  RtcLostTime, CorruptImage
- `record_fault()` / `last_fault()` / `fault_count()` - stored in backup registers
- `ResetReason` - why the device last reset
- `check_reset_reason()` - reads the hardware reset cause at boot
- `check_boot_throttle()` - detects a brown-out reboot loop and drops into a safe
  state
- `signal_fault()` - LED flash code (N red flashes = fault N)

**Fault codes** (shown as N red LED flashes, and stored for the diagnostics
face to display after a reset):

| Code | Fault | Meaning |
|------|-------|---------|
| 1 | WatchdogReset | A hang occurred, the hardware watchdog reset the watch |
| 2 | Panic | A software panic (bug), recorded before the reset |
| 3 | WakeTooLong | A wake event took too long to process |
| 4 | InvalidState | An invalid event or state was encountered |
| 5 | BatteryLow | The battery is critically low |
| 6 | RtcLostTime | The RTC lost time (crystal issue or frozen clock) |
| 7 | CorruptImage | The firmware image failed its CRC integrity check |
| 8 | ClockFailure | The 32 kHz crystal failed, RTC on the internal oscillator |

**Why:** when something goes wrong, the watch tells the user via LED codes
instead of silently failing. Faults are stored in **fixed** backup registers
(no growth).

**Why boot throttling:** when a battery drops below ~2.0 V, a high-load peripheral
can pull the rail below the CPU threshold, resetting the chip, which then reboots
into the same load - an infinite loop. Counting boots in a short window and
entering a safe state (buzzer/LED disabled) breaks the loop.

### 7.5 `movement/persist.rs` - Settings persistence

Saves settings to flash (with a magic value) so they survive reset, and loads
them back on boot. Uses the wear-leveled storage.

### 7.6 `movement/board.rs` - Board configuration

Stores the board type (green/red/blue/pro) and buzzer voltage. `apply()` applies
the config to hardware (LED polarity, buzzer voltage). Buzzer drive is board-aware:
Red / Lite, Green, and Blue remain battery-level/unboosted, while 9 V boosted drive
is reserved for the explicitly supported Pro converter revision and unknown inputs
fail closed.

**Why:** a freshly-flashed watch can be configured on-device (via the diagnostics
face) without recompiling.

### 7.7 `movement/stats.rs` - Statistics tracking

Tracks button presses per button and buzzer rings, stored in backup registers so
they survive reset.

### 7.8 `movement/battery.rs` - Battery configuration

Stores the installed battery type (CR2012/2016/2025/2032/2050) and estimates the
remaining charge and days of life from the measured voltage. Configured from the
diagnostics face's battery submenu.

### 7.9 The watch faces

Each face implements the `WatchFace` trait. Faces are `static` instances (no
heap). They are pure state machines: they react to one event and return.

There are **111 registered faces** (`MOVEMENT_NUM_FACES = 111`), covering all faces
from the original reference repo plus the Second Movement faces. Highlights:

- `simple_clock.rs` - the main clock (weekday, day, time, seconds toggle)
- `countdown.rs` - a countdown timer (scheduled via RTC alarm)
- `alarm.rs` - alarms with day/hour/minute/pitch/beeps settings
- `counter.rs` - a tally counter (0-99)
- `world_clock.rs` / `world_clock2.rs` - time in a selected timezone
- `diagnostics.rs` - a task/device/storage manager with a hierarchical menu
- `advanced_alarm.rs` - 16 alarm slots with day modes, pitch, and beep rounds
- `hydration.rs` - water intake tracking with settings and a log
- `sos.rs` - SOS / Morse code transmitter
- `lander.rs` / `ping.rs` / `blackjack.rs` / `squash.rs` - games
- `tide.rs` / `days_since.rs` / `baby_kicks.rs` - complications
- `settings_face.rs` - unified settings
- `ish.rs` / `solar_time.rs` / `ke_decimal_time.rs` / `beats.rs` - alternative clocks
- Plus stopwatch, timer, moon phase, calculators, astronomy, and many more

---

## 8. The `core` crate (pure logic)

The `core` crate contains pure computation with no hardware dependency, so it can
be unit-tested on the host.

### 8.1 `core/datetime.rs`

The `DateTime` type and its pack/unpack logic. This is the same type used by the
RTC, but defined here so it can be tested.

### 8.2 `core/settings.rs`

The `Settings` bit-packing getters/setters. Tested for round-trip correctness.

### 8.3 `core/utility.rs`

Date/time math: leap years, weekdays, week numbers, UNIX time conversion,
durations, 12-hour conversion. These are the musl-derived algorithms, ported
carefully.

### 8.4 `core/uf2.rs`

UF2 block encoding, used by the `uf2tool` binary to convert the raw firmware
binary into a `.uf2` file for drag-and-drop flashing.

**Why a separate crate:** these functions have no hardware dependency, so they
can run on a desktop and be unit-tested. The tests caught a real `is_leap` bug.

---

## 9. The `panic.rs` module

The panic handler blinks the LED 3 times (a visible fault indicator) then resets
the device. This ensures the watch recovers from a software bug instead of
freezing forever. The release profile sets `panic = "abort"` to strip unwind
tables and keep the binary small.

**Why reset-on-panic:** a sealed, air-gapped wearable must recover on its own. A
frozen watch is useless, a resetting watch recovers.

---

## 10. Key design decisions and rationale

### 10.1 Why zero-heap?

A heap is a source of unbounded memory growth and allocation failures. On a
sealed wearable, that's unacceptable. By making every face a `static` instance,
memory growth is **structurally impossible** - the compiler won't even let you
allocate.

### 10.2 Why the event-driven dispatcher instead of a polling loop?

The original C firmware polls. Polling keeps the CPU awake, wasting power. The
event-driven model wakes the CPU only to react, then sleeps. This is the single
biggest power win.

### 10.3 Why does the RTC own all timekeeping?

The RTC keeps counting in STANDBY. So the CPU can sleep and the RTC tracks
elapsed time. This is what makes a running stopwatch/timer possible without the
CPU staying awake.

### 10.4 Why bounded timeouts on all polling loops?

A `while !flag {}` with no bound means a hung peripheral hangs the CPU. With a
timeout, the driver returns an error instead. Combined with the watchdog, the
system is un-hangable.

### 10.5 Why the watchdog?

The panic handler catches panics, but not hangs. The watchdog catches both -
if the main loop stops completing, the WDT resets the chip.

### 10.6 Why wear-leveled, write-verified storage?

Flash has limited write cycles. Wear leveling spreads writes across rows.
write-verify confirms each write succeeded. This protects persistent settings.

### 10.7 Why RAM for state, flash only for settings?

RAM is always-on in STANDBY and holds runtime state (fast, no wear, survives
face switches). Flash holds only truly-persistent settings that must survive a
full power-off, written rarely and wear-leveled.

### 10.8 Why the closed `Event` enum?

A closed enum means every possible event is known and handled. No ambiguous
states, no unexpected inputs. This makes the system deterministic.

### 10.9 Why mutable settings in the face trait?

Faces need to change settings (e.g. the alarm face toggles `alarm_enabled`). The
trait passes `&mut Settings` so faces can update settings, which are then
auto-persisted after each reaction.

### 10.10 Why the hardware hardening?

A sealed, air-gapped watch must survive silicon quirks, bit-rot, and power
failures on its own. Each hardening measure addresses a specific failure mode:

| Failure mode | Mitigation |
|--------------|------------|
| Standby-entry hard fault (SysTick race) | Disable SysTick around `wfi()` |
| Sensor board backward-powering via I2C | Float the I2C pins before sleep |
| Inverted battery ADC curve | Inverse scaling in `get_vcc_voltage()` |
| Brown-out reboot loop | BOD33 + boot-count throttle |
| Read-while-write bus stall | `.ramfunc` flash writes |
| Flash alignment faults | `#[repr(C, align(4))]` on flash structs |
| Interrupt livelock masking a hang | Windowed watchdog clearing |
| Flash bit-rot | CRC-32 integrity check |
| Panic code bloat | `panic = "abort"` |

---

## 11. Testing and CI

The project has a CI pipeline (GitHub Actions) that runs on every push:

- **Debug validation** - `cargo check --target thumbv6m-none-eabi -p sensor-watch`
- **Release link** - `cargo build --release --target thumbv6m-none-eabi -p sensor-watch`
- **Clippy** - informational firmware clippy for `thumbv6m-none-eabi`, plus
  strict core clippy with `-D warnings`
- **Test** - `cargo test -p sensor-watch-core` (host unit tests)
- **Format** - `cargo fmt --check`
- **Studio** - build the GUI companion app
- **UF2** - assemble the firmware `.uf2` artifact

The core crate currently has 68 passing host tests covering date math, settings
bit-packing, DateTime pack/unpack, UF2 encoding, ECC, event logging, transfer
validation, optical protocol validation, and other pure logic. These tests
caught a real `is_leap` bug. Passing host tests do not validate physical hardware.

**Why CI:** it gives *proof* the foundation is correct and prevents regressions.
Every claim about correctness is verified automatically.

---

## 12. Glossary

| Term | Meaning |
|------|---------|
| **HAL** | Hardware Abstraction Layer - the `watch` module wrapping peripherals |
| **PAC** | Peripheral Access Crate - `atsaml22j`, low-level register access |
| **RTC** | Real-Time Clock - keeps time, owns all timekeeping |
| **WDT** | Watchdog Timer - resets the chip on a hang |
| **EIC** | External Interrupt Controller - handles button presses |
| **BOD33** | Brown-Out Detector - monitors VDD, interrupts on low battery |
| **STANDBY** | A low-power state where the CPU sleeps but RAM/LCD/RTC stay on |
| **BACKUP** | The deepest sleep - RAM and CPU off, only RTC runs |
| **RWW** | Read-While-Write - the EEPROM emulation area |
| **UF2** | The bootloader format for drag-and-drop firmware updates |
| **Wear leveling** | Rotating flash writes to avoid wearing out one row |
| **CRC** | Cyclic Redundancy Check - detects data corruption |
