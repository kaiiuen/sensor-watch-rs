# Community and Upstream Credits

This project is a Rust rewrite and integration effort built on the work of the
Sensor-Watch and Second Movement communities. The names and projects below are
credited for upstream firmware, watch faces, hardware experiments, tooling,
protocols, testing, documentation, and ideas that informed this project.

## Upstream projects and maintainers

- **Joey Castillo / `joeycastillo`** - Sensor Watch author and maintainer;
  Movement architecture, watch library, simulator, hardware design, display
  work, power investigations, documentation, review, and project coordination.
- **Second Movement contributors** - UTC/UTZ timekeeping, DST, custom LCDs,
  board variants, face ports, background-task APIs, alarm/tune work, USB/UART
  experiments, and hardware validation.
- **`evq`** - `utz`, the timezone/DST library used as an upstream reference.
- **`atsamd-rs` and `svd2rust` contributors** - PAC, HAL, and register-generation
  work that informed SAM L22 Rust support.
- **Microchip** - SAM L22 datasheets and silicon errata.
- **STMicroelectronics** - LIS2DW/LIS2DW12/LIS2DUX12 sensor documentation.

## Named community contributors

The following contributors were explicitly credited, thanked, or associated
with work in the community discussion export. Attribution is intentionally
high-level where the export did not establish a single commit owner.

- **ZeptoBars / BarsMonster** - precision timing, frequency correction,
  temperature compensation, RTC investigations, power profiling, and UltraPatch.
- **Tahnok** - watch faces, framework work, background tasks, testing, and
  simulator discussions.
- **WJHRDY** - Wyoscan face and low-energy animation work.
- **Neutralinsomniac** - Smallchess/chess face and engine integration.
- **Austen Adler / `austenadler`** - early Rust integration experiments.
- **Wesleyac** - link-time optimization and review/merge assistance.
- **Matheus Moreira** - large feature integration branch, structured TOTP,
  deadline/USB/clock work, testing coordination, and preserved attribution.
- **Voloved / Devolov / `devolov`** - DST/UTZ, sunrise/sunset, step-count,
  quiet-hours, LED, battery, and display work.
- **Krzysztof Gałka / `kshysztof`** - debounce and hardware button testing.
- **Atax1a** - hardware testing, silicon-errata work, and development support.
- **Osresearch / Trammell Hudson** - MicroPython porting and power-analysis
  investigations.
- **Alessandro Genova / `alesgenova`** - Counter32, fast stopwatch, optical
  communications, UltraPatch integration, location faces, and sensor work.
- **Ruben Sandwich** - custom display, step-count experimentation, and
  hardware testing.
- **`knrd`** - step-count algorithms and benchmark testing.
- **Gabor / Gugray / `eiriksm` / `soundblaster`** - Chirpy/Fesk acoustic
  communications, receiver tools, tone selection, and protocol testing.
- **Jim di Griz** - battery-drain and low-voltage investigations.
- **Faldor20** - dive-computer and pressure-sensor experiments.
- **Nima Kalantar** - prayer-times face work.
- **Ucodia** - Flowtime face.
- **Ganapati** - custom faces and metronome work.
- **Aron Hegedus** - Sea Shanty face.
- **Alessandro and community testers** - dynamic tunes, hourly chimes, and
  acoustic-transfer experiments.
- **James / `wryun`** - calculator, builder, and simulator/tooling discussions.
- **Jeremy** - custom-display simulator and build integration.
- **Fgergo, Crim, Jack, Alexis Philip, Michael Shriver, Benny Blue, Monican,
  Cyberdeath, Agent-E11, and many other contributors** - faces, TOTP/HOTP,
  display mappings, Rust/Zig experiments, builders, documentation, and review.

## Community tools and integrations

- `sensor-watch-ir-tools` and the community IrDA/optical flashing work.
- UltraPatch and `detools` research for small in-place Cortex-M updates.
- ChirpyRX and Fesk receiver prototypes for acoustic data transfer.
- `edbg`, OpenOCD, GDB, J-Link, Raspberry Pi Debug Probe, and SWD workflows.
- Nordic Power Profiler Kit 2, Joulescope, EnergyTrace, and bench-current
  measurement workflows.
- Emscripten browser simulator and custom-LCD display tooling.
- LittleFS, USB mass-storage experiments, and host filesystem utilities.
- `utz`, `gossamer`, `smallchesslib`, `nanopb`/protobuf discussions, and
  embedded sensor reference projects.

## Attribution policy

This repository does not claim ownership of upstream C code, community faces,
third-party libraries, hardware designs, or experimental protocols. The
reference repositories remain available under their own licenses in the local
workspace. New ports or adaptations should retain the original license and
attribution headers, link the upstream source or PR when practical, and add a
focused note here when a community contribution becomes part of Rust Studio or
the firmware.

The Discord export used for this inventory is private project research and is
not included in this repository or its commits.
