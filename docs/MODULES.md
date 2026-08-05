# Sensor-Watch Firmware — Module Reference

This document is a **symbol-by-symbol reference** for every module in the
firmware. For each public item, it explains what it is, why it exists, and how
it's used. It complements `ARCHITECTURE.md` (the high-level overview).

---

## `src/main.rs` — Entry point

| Symbol | Type | Purpose |
|--------|------|---------|
| `main()` | fn | The reset handler. Runs the boot sequence, then the event loop. |
| `copy_ramfunc()` | fn | Copies the `.ramfunc` section from flash to RAM at boot. |
| `#![no_std]` | attr | No standard library (bare-metal). |
| `#![no_main]` | attr | No `main` symbol from std; we define our own entry. |
| `#![allow(static_mut_refs)]` | attr | Allows access to `static mut` (used for global state). |

**Boot sequence rationale:** each step depends on the previous. RAM-copy →
reset-reason → boot-throttle → interrupt priorities → clocks → RTC → watchdog →
BOD33 → framework → faces → tick → loop.

---

## `src/panic.rs` — Panic handler

| Symbol | Type | Purpose |
|--------|------|---------|
| `panic()` | fn | The `#[panic_handler]`. Blinks the LED 3x, then resets. |
| `delay()` | fn | A crude blocking delay for the blink. |

**Why reset-on-panic:** a sealed wearable must recover on its own. Blinking
first gives a visible fault indicator; resetting recovers. The release profile
uses `panic = "abort"` to keep the binary small.

---

## `src/watch/mod.rs` — HAL root

| Symbol | Type | Purpose |
|--------|------|---------|
| `init()` | fn | Dependency-ordered hardware init (irq → clock → rtc → wdt). |

---

## `src/watch/irq.rs` — Interrupt priorities

| Symbol | Type | Purpose |
|--------|------|---------|
| `init()` | fn | Sets NVIC priorities for RTC, EIC, TC3. |

**Why priorities:** the RTC alarm (0) must never be blocked by a button (2) or
tick (1). Lower value = higher urgency on ARMv6-M.

---

## `src/watch/clock.rs` — Clock init

| Symbol | Type | Purpose |
|--------|------|---------|
| `init()` | fn | Enables XOSC32K, routes 1 kHz to RTC, enables RTC APB clock. |
| `init_xosc32k()` | fn | Enables the 32 kHz crystal. |
| `init_rtc_source()` | fn | Selects the RTC clock source. |
| `enable_rtc_apb()` | fn | Enables the RTC's APB clock in MCLK. |

**Why:** the RTC needs a clock source before it can run.

---

## `src/watch/rtc.rs` — Real-Time Clock

| Symbol | Type | Purpose |
|--------|------|---------|
| `DateTime` | struct | Packed date/time (second/minute/hour/day/month/year). |
| `DateTime::to_reg()` | fn | Packs fields into the 32-bit RTC register value. |
| `DateTime::from_reg()` | fn | Unpacks a register value into a `DateTime`. |
| `AlarmMatch` | enum | Which alarm components to match (SS/MMSS/HHMMSS). |
| `Callback` | type | A function pointer for callbacks. |
| `init()` | fn | Configures the RTC in clock/calendar mode. |
| `is_enabled()` | fn | Returns true if the RTC is enabled. |
| `set_date_time()` | fn | Sets the clock (double-sync for reliability). |
| `get_date_time()` | fn | Reads the clock. |
| `register_tick_callback()` | fn | 1 Hz tick callback. |
| `register_periodic_callback()` | fn | Configurable tick (1-128 Hz). |
| `register_alarm_callback()` | fn | Alarm at a specific time. |
| `schedule_wakeup()` | fn | One-shot wakeup at a time. |
| `schedule_wakeup_in()` | fn | One-shot wakeup N seconds from now. |
| `enable()` | fn | Enable/disable the RTC. |
| `freqcorr_write()` | fn | Crystal drift correction. |
| `RTC()` | fn | The interrupt handler (tick/tamper/alarm dispatch). |

**Why the RTC owns timekeeping:** it keeps counting in STANDBY, so the CPU can
sleep while the RTC tracks elapsed time. All register writes wait for `SYNCBUSY`.

---

## `src/watch/slcd.rs` — Segment LCD

| Symbol | Type | Purpose |
|--------|------|---------|
| `Indicator` | enum | The 5 indicator segments (Signal, Bell, Pm, H24, Lap). |
| `enable_display()` | fn | Init + enable the SLCD. |
| `set_pixel()` / `clear_pixel()` | fn | Direct segment control. |
| `display_character()` | fn | Render one character at a position. |
| `display_string()` | fn | Render a string. |
| `set_colon()` / `clear_colon()` | fn | The colon segment. |
| `set_indicator()` / `clear_indicator()` | fn | Indicator segments. |
| `start_character_blink()` | fn | Autonomous blink in position 7. |
| `start_tick_animation()` | fn | Autonomous tick-tock animation. |
| `CHARACTER_SET` | const | ASCII → 7-segment bit patterns. |
| `SEGMENT_MAP` | const | Position → (common, segment) pins. |

**Why:** the LCD retains its display in STANDBY with no CPU, so the time stays
visible while the CPU sleeps.

---

## `src/watch/gpio.rs` — GPIO

| Symbol | Type | Purpose |
|--------|------|---------|
| `Pin` | struct | A `(port, pin)` pair. |
| `Direction` | enum | Off, In, Out. |
| `PullMode` | enum | Off, Up, Down. |
| `Function` | enum | Off, A, Mux(u8) — peripheral function. |
| `set_pin_direction()` | fn | Set direction. |
| `set_pin_pull_mode()` | fn | Set pull. |
| `set_pin_function()` | fn | Set peripheral function. |
| `get_pin_level()` | fn | Read input level. |
| `set_pin_level()` | fn | Write output level. |

**Why `Function::Mux(u8)`:** peripherals like TCC/ADC need specific PMUX values
(e.g. 5 for TCC, 1 for ADC), not just "A".

---

## `src/watch/extint.rs` — Buttons / EIC

| Symbol | Type | Purpose |
|--------|------|---------|
| `BTN_ALARM` / `BTN_LIGHT` / `BTN_MODE` | const | The button pins. |
| `A0`-`A4` | const | The 9-pin connector pins. |
| `Trigger` | enum | None, Rising, Falling, Both. |
| `Callback` | type | Function pointer. |
| `enable_external_interrupts()` | fn | Enable the EIC. |
| `register_interrupt_callback()` | fn | Set up a button interrupt. |
| `EIC()` | fn | The interrupt handler. |

**Why:** the buttons are the core input UI. They wake the watch from STANDBY.

---

## `src/watch/led.rs` — LED

| Symbol | Type | Purpose |
|--------|------|---------|
| `enable_leds()` | fn | Enable TCC0 PWM. |
| `set_led_color()` | fn | Set red/green duty cycle. |
| `set_led_color_rgb()` | fn | Set red/green/blue duty cycle. |
| `set_led_red()` / `set_led_green()` / `set_led_off()` | fn | Presets. |
| `set_invert_polarity()` | fn | Invert for common-anode (Red/Pro) boards. |
| `is_enabled()` | fn | Is the TCC on. |

**Why PWM:** arbitrary brightness/color. TCC outputs hold state in STANDBY.

---

## `src/watch/buzzer.rs` — Buzzer

| Symbol | Type | Purpose |
|--------|------|---------|
| `Note` | enum | Musical notes A1-B8 + Rest. |
| `NOTE_PERIODS` | const | Period for each note (1 MHz / freq). |
| `enable_buzzer()` | fn | Enable TCC0. |
| `set_buzzer_period()` | fn | Set the tone period. |
| `set_buzzer_on()` / `set_buzzer_off()` | fn | Output control. |
| `play_note()` | fn | Play a note (blocking). |
| `play_sequence()` | fn | Play a note sequence (non-blocking, TC3). |
| `set_voltage()` | fn | Set buzzer voltage. |

---

## `src/watch/adc.rs` — ADC

| Symbol | Type | Purpose |
|--------|------|---------|
| `ReferenceVoltage` | enum | INTREF, VCC/1.6, VCC/2, VCC. |
| `enable_adc()` | fn | Enable the ADC. |
| `get_analog_pin_level()` | fn | Read a pin. |
| `get_vcc_voltage()` | fn | Battery voltage in mV (inverse scaling). |
| `set_analog_num_samples()` | fn | Sample count. |
| `set_analog_reference_voltage()` | fn | Reference selection. |

**Why inverse scaling:** the battery rail is measured against the internal
reference, so the raw value *rises* as the battery weakens. The formula
`V = 1.0 V * ADC_Max / ADC_Raw` corrects for this.

---

## `src/watch/i2c.rs` — I2C

| Symbol | Type | Purpose |
|--------|------|---------|
| `enable_i2c()` | fn | Enable SERCOM1 I2C master. |
| `send()` / `receive()` | fn | Raw transfers. |
| `write8()` / `read8()` / `read16()` / `read24()` / `read32()` | fn | Register helpers. |
| `pins_to_floating_before_sleep()` | fn | Float SDA/SCL to stop sensor leakage. |

---

## `src/watch/spi.rs` — SPI

| Symbol | Type | Purpose |
|--------|------|---------|
| `enable_spi()` | fn | Enable SERCOM3 SPI master. |
| `write()` / `read()` / `transfer()` | fn | Data transfers. |

---

## `src/watch/uart.rs` — UART

| Symbol | Type | Purpose |
|--------|------|---------|
| `enable_uart()` | fn | Enable SERCOM3 USART. |
| `puts()` | fn | Transmit a string. |
| `getc()` | fn | Receive a byte. |

---

## `src/watch/storage.rs` — Flash storage

| Symbol | Type | Purpose |
|--------|------|---------|
| `read()` / `write()` / `erase()` | fn | Raw RWW EEPROM access. `write()`/`erase()` run from `.ramfunc`. |
| `sync()` | fn | Wait for pending writes. |
| `wear_leveled_write()` | fn | Write with wear leveling (rotates rows). |

**Why wear leveling:** flash has limited write cycles; rotating extends life.
**Why `.ramfunc`:** avoids the read-while-write bus stall.

---

## `src/watch/deepsleep.rs` — Sleep control

| Symbol | Type | Purpose |
|--------|------|---------|
| `register_extwake_callback()` | fn | External wake on A2/A4/Alarm. |
| `store_backup_data()` / `get_backup_data()` | fn | RTC backup registers. |
| `enter_sleep_mode()` | fn | Enter STANDBY. |
| `enter_standby()` | fn | SysTick-safe STANDBY (main-loop sleep). |
| `init_bod33()` | fn | Configure the brown-out detector. |
| `enter_backup_mode()` | fn | Enter the deepest sleep (BACKUP). |

---

## `src/watch/timeout.rs` — Bounded waits

| Symbol | Type | Purpose |
|--------|------|---------|
| `Error` | enum | Timeout, Bus, InvalidArgument. |
| `wait_until()` | fn | Wait with a timeout, return `Result`. |

**Why:** a bounded wait means a hung peripheral returns an error instead of
hanging the CPU.

---

## `src/watch/wdt.rs` — Watchdog

| Symbol | Type | Purpose |
|--------|------|---------|
| `init()` | fn | Enable the WDT (~2s, always-on). |
| `kick()` | fn | Reload the WDT. |
| `kick_windowed()` | fn | Reload only from the main loop (windowed). |

**Why:** catches hangs that the panic handler can't.

---

## `src/watch/crc.rs` — CRC-32 integrity

| Symbol | Type | Purpose |
|--------|------|---------|
| `crc32()` | fn | Compute CRC-32 over a flash range. |
| `check_firmware_integrity()` | fn | Compare the text CRC against the expected signature. |

**Why:** detects flash bit-rot so a corrupt image can be caught.

---

## `src/watch/ecc.rs` — SECDED error correction

| Symbol | Type | Purpose |
|--------|------|---------|
| `encode()` | fn | Encode a 32-bit word with a 7-bit Hamming SECDED code. |
| `decode()` | fn | Decode, correcting single-bit and detecting double-bit errors. |

---

## `src/watch/shell.rs` — Serial command shell

| Symbol | Type | Purpose |
|--------|------|---------|
| `Shell` | struct | A minimal command interpreter over UART. |
| `Shell::poll()` | fn | Read and process incoming commands. |

Commands: `time`, `settime YYMMDDHHMMSS`, `help`.

---

## `src/watch/memory.rs` — Memory usage

| Symbol | Type | Purpose |
|--------|------|---------|
| `total_ram()` | fn | Total RAM available (32 KB). |
| `static_ram_used()` | fn | Static RAM used (`.data` + `.bss`). |
| `ram_used_percent()` | fn | RAM usage as a percentage. |

---

## `src/watch/utility.rs` — Utility

| Symbol | Type | Purpose |
|--------|------|---------|
| `get_weekday()` | fn | Weekday abbreviation. |
| `get_iso8601_weekday_number()` | fn | Monday=1..Sunday=7. |
| `get_weeknumber()` | fn | Week number (1-53). |
| `is_leap()` | fn | Leap year check. |
| `days_since_new_year()` | fn | Day of year. |
| `convert_to_unix_time()` | fn | Date → UNIX time. |
| `date_time_from_unix_time()` | fn | UNIX time → date. |
| `seconds_to_duration()` | fn | Seconds → days/hours/min/sec. |
| `convert_to_12_hour()` | fn | 12-hour conversion. |

---

## `src/watch/utz.rs` / `watch/zones.rs` — DST timezones

| Symbol | Type | Purpose |
|--------|------|---------|
| `dayofweek()` | fn | Day of week (Monday=1..Sunday=7). |
| `is_leap_year()` | fn | Leap year check (year since 2000). |
| `get_current_offset()` | fn | Effective offset applying DST rules. |
| `unpack_zone()` | fn | Unpack a packed zone definition. |
| `ZONE_RULES` / `ZONE_DEFNS` / `ZONE_NAMES` | const | The 46-zone DST table. |

---

## `src/watch/lis2dw.rs` — LIS2DW accelerometer

| Symbol | Type | Purpose |
|--------|------|---------|
| `begin()` | fn | Initialize the accelerometer. |
| `get_raw_reading()` | fn | Read a raw 3-axis sample. |
| `set_data_rate()` / `set_mode()` / `set_range()` | fn | Configuration. |
| `configure_tap_threshold()` / `configure_tap_duration()` | fn | Tap detection. |
| `enable_double_tap()` / `disable_double_tap()` | fn | Tap control. |
| `get_interrupt_source()` | fn | Read the interrupt source register. |

---

## `src/watch/rtc.rs` — Compare-callback queue

| Symbol | Type | Purpose |
|--------|------|---------|
| `N_COMP_CB` | const | Number of compare slots (8). |
| `register_comp_callback()` | fn | Register a compare callback at a target time. |
| `disable_comp_callback()` | fn | Disable a compare callback. |
| `schedule_wakeup()` | fn | One-shot alarm wakeup. |

---

## `src/movement/types.rs` — Core types

| Symbol | Type | Purpose |
|--------|------|---------|
| `MOVEMENT_NUM_FACES` | const | Number of face slots (111). |
| `MOVEMENT_LONG_PRESS_TICKS` | const | Long-press threshold (64 fast ticks). |
| `MOVEMENT_REALLY_LONG_PRESS_TICKS` | const | Really-long-press threshold (192 fast ticks). |
| `Settings` | struct | 32-bit packed user preferences, `#[repr(C, align(4))]`. |
| `Button` | enum | Light, Mode, Alarm. |
| `ButtonEvent` | enum | Down, Up, LongPress, LongUp, ReallyLongPress. |
| `Event` | enum | Activate, Tick, BackgroundTask, Button. |
| `WatchFace` | trait | The face interface (setup/activate/loop_/resign + optional hooks). |
| `MovementState` | struct | Global framework state. |
| `ClockMode` | enum | 12H / 24H / 024H. |
| `BuzzerPriority` | enum | Button / Signal / Alarm. |
| `MOVEMENT_SECONDARY_FACE_INDEX` | const | Start of the secondary face list. |
| `TIMEZONE_OFFSETS` | const | Timezone table (41 entries). |

**Why a closed `Event` enum:** deterministic — every event is known and handled.

---

## `src/movement/mod.rs` — Dispatcher

| Symbol | Type | Purpose |
|--------|------|---------|
| `app_init()` | fn | Load settings, init state. |
| `app_setup()` | fn | Register faces, buttons, alarms. |
| `app_loop()` | fn | React to one event, then return. |
| `set_tick_rate()` | fn | 1 Hz vs 1/minute wake rate. |
| `move_to_face()` / `move_to_next_face()` | fn | Face switching. |
| `default_loop_handler()` | fn | Standard button behavior. |
| `save_settings()` / `store_settings()` | fn | Persist settings. |
| `release_peripherals()` | fn | Disable unused peripherals. |
| `get_local_date_time()` etc. | fn | Timezone-aware time accessors. |
| `cb_tick()` | fn | 1 Hz tick callback. |
| `cb_fast_tick()` | fn | 128 Hz fast tick (long-press). |
| `cb_light_btn_interrupt()` etc. | fn | Button interrupt callbacks. |

---

## `src/movement/debounce.rs` — Debouncing

| Symbol | Type | Purpose |
|--------|------|---------|
| `update()` | fn | Feed a raw pin reading, get a debounced event. |
| `check_long_press()` | fn | Detect long/really-long presses on the fast tick. |

**Why:** mechanical buttons bounce; debouncing filters spurious edges.

---

## `src/movement/fault.rs` — Fault system

| Symbol | Type | Purpose |
|--------|------|---------|
| `Fault` | enum | WatchdogReset, Panic, WakeTooLong, InvalidState, BatteryLow, RtcLostTime, CorruptImage, ClockFailure. |
| `ResetReason` | enum | PowerOn, Watchdog, Panic, Software. |
| `record_fault()` | fn | Record a fault in a backup register. |
| `last_fault()` / `fault_count()` | fn | Read fault state. |
| `check_reset_reason()` | fn | Read hardware reset cause at boot. |
| `check_boot_throttle()` | fn | Detect a brown-out reboot loop. |
| `check_clock_failure()` | fn | Detect a broken 32 kHz crystal. |
| `check_heartbeat()` | fn | Detect a frozen RTC (seconds not advancing). |
| `signal_fault()` | fn | LED flash code. |

---

## `src/movement/persist.rs` — Persistence

| Symbol | Type | Purpose |
|--------|------|---------|
| `load()` | fn | Load settings from flash. |
| `save()` | fn | Save settings to flash (wear-leveled). |

---

## `src/movement/board.rs` — Board config

| Symbol | Type | Purpose |
|--------|------|---------|
| `Board` | enum | Green, Red, Blue, Pro. |
| `BoardConfig` | struct | Board type + buzzer voltage. |
| `read()` / `write()` | fn | Read/write from backup register. |
| `apply()` | fn | Apply to hardware (LED polarity, buzzer voltage). |

---

## `src/movement/stats.rs` — Statistics

| Symbol | Type | Purpose |
|--------|------|---------|
| `Stats` | struct | Button presses, buzzer rings, etc. |
| `read()` | fn | Read counters. |
| `press_light()` / `press_mode()` / `press_alarm()` | fn | Increment button counters. |
| `buzzer_ring()` | fn | Increment buzzer counter. |

---

## `src/movement/battery.rs` — Battery config

| Symbol | Type | Purpose |
|--------|------|---------|
| `BatteryType` | enum | CR2012, CR2016, CR2025, CR2032, CR2050. |
| `battery_type()` / `set_battery_type()` | fn | Read/write the configured battery type. |
| `charge_percent()` | fn | Estimate remaining charge from voltage. |
| `days_remaining()` | fn | Estimate days of life from voltage + capacity. |

---

## The watch faces

Each face implements `WatchFace` with `setup`, `activate`, `loop_`, `resign`,
and optionally `wants_background_task` and `advise`.

There are **111 registered faces**. Highlights:

| Face | File | Purpose |
|------|------|---------|
| Simple clock | `simple_clock.rs` | Main clock, seconds toggle. |
| Countdown | `countdown.rs` | Countdown timer. |
| Alarm | `alarm.rs` | Alarms with settings. |
| Advanced alarm | `advanced_alarm.rs` | 16 alarm slots with day modes, pitch, beeps. |
| Counter | `counter.rs` | Tally counter. |
| World clock | `world_clock.rs` / `world_clock2.rs` | Timezone clocks. |
| Diagnostics | `diagnostics.rs` | Device/settings/stats manager. |
| Hydration | `hydration.rs` | Water intake tracker with log. |
| SOS | `sos.rs` | Morse code transmitter. |
| Lander | `lander.rs` | Lunar lander game. |
| Ping | `ping.rs` | Pong-style game. |
| Blackjack | `blackjack.rs` | Blackjack game. |
| Squash | `squash.rs` | Squash game. |
| Tide | `tide.rs` | Tide prediction. |
| Days since | `days_since.rs` | Days-since counter. |
| Baby kicks | `baby_kicks.rs` | Kick counter. |
| Settings | `settings_face.rs` | Unified settings screens. |
| ISH | `ish.rs` | Vague time. |
| Solar time | `solar_time.rs` | Solar time. |
| Kè decimal | `ke_decimal_time.rs` | Decimal time. |
| Beats | `beats.rs` | Swatch Internet Time. |
| Stopwatch | `stopwatch.rs` | Stopwatch. |
| Timer | `timer.rs` | Multi-slot countdown timer. |
| Moon phase | `moon_phase.rs` | Lunar phase. |
| + 74 more | — | Games, calculators, astronomy, sensors, and more. |

---

## The `core` crate

| Module | Contents |
|--------|----------|
| `datetime.rs` | `DateTime` + pack/unpack. |
| `settings.rs` | `Settings` bit-packing. |
| `utility.rs` | Date/time math. |
| `uf2.rs` | UF2 encoding (used by the build tool). |

The `core` crate is host-testable (no hardware dependency), so its logic is
covered by unit tests.
