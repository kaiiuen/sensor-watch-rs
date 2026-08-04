# Sensor-Watch (Rust)

A from-scratch **Rust rewrite** of the [Sensor-Watch](https://github.com/joeycastillo/Sensor-Watch)
firmware for the Microchip **SAM L22J18A** (ARM Cortex-M0+), the board replacement
for the classic Casio F-91W.

The goal is to reimplement the entire firmware in Rust — the hardware abstraction
layer, the watchface framework, and the watch faces — and then extend it with new
faces and features.

## Project layout

```
sensor-watch-rs/          <- THIS project (the Rust rewrite)
sensor-watch-reference/   <- clone of the original C repo (reference + docs only)
```

The original C source is kept in `sensor-watch-reference/` purely as a reference
for behavior, register maps, and documentation. We do not modify it.

## Hardware

- MCU: Microchip SAM L22J18A (ARM Cortex-M0+)
- 10-digit segment LCD + 5 indicator segments
- 3 interrupt-capable buttons
- Red/green PWM LED backlight
- Optional piezo buzzer
- 32 kHz crystal RTC with alarm
- USB (UF2 bootloader)

### Memory map (from the reference linker script)

| Region   | Address        | Size      |
|----------|----------------|-----------|
| Bootloader| 0x00000000    | 0x2000    |
| Firmware | 0x00002000     | 0x3A000   |
| EEPROM   | 0x0003C000     | 0x2000    |
| RAM      | 0x20000000     | 0x8000    |

## Building

Prerequisites:

- Rust stable with the `thumbv6m-none-eabi` target:
  ```
  rustup target add thumbv6m-none-eabi
  ```

Build:

```
cargo build
```

The firmware links against `memory.x` and produces an ELF in
`target/thumbv6m-none-eabi/<profile>/sensor-watch`.

## Status

- [x] Project skeleton + toolchain (builds for `thumbv6m-none-eabi`)
- [ ] Watch library (LCD, RTC, buttons, LED, buzzer, I2C/SPI/UART)
- [ ] Watchface framework
- [ ] Watch faces
- [ ] New faces & features

## License

MIT OR Apache-2.0 (this rewrite). The reference C project has its own license;
see `sensor-watch-reference/LICENSE.md`.
