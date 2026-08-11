# Sensor-Watch Firmware - Getting Started & Contributing

This guide covers how to build, test, and contribute to the Sensor-Watch firmware
rewrite in Rust.

---

## Prerequisites

- **Rust** (stable) with the `thumbv6m-none-eabi` target:
  ```sh
  rustup target add thumbv6m-none-eabi
  ```
- **Git** for version control.

## Building

The firmware builds for the SAM L22 (Cortex-M0+):

```sh
cargo build --target thumbv6m-none-eabi
```

For a release build (optimized, smaller):

```sh
cargo build --release --target thumbv6m-none-eabi
```

The output ELF is at `target/thumbv6m-none-eabi/release/sensor-watch`.

## Testing

The `core` crate contains pure logic that can be tested on the host:

```sh
cargo test -p sensor-watch-core --target x86_64-pc-windows-msvc
```

(On Linux/macOS, use `x86_64-unknown-linux-gnu` or `x86_64-apple-darwin`.)

The source tree currently contains 189 `#[test]` attributes across core,
firmware host seams, and Studio. The host test command exercises date math,
settings bit-packing, DateTime pack/unpack, UF2 encoding, event logging, and
other host seams, but the current checkout is blocked by the existing `vec!`
macro import error in `core/src/uf2.rs`. Passing host tests provide confidence
in pure logic; they do not validate physical hardware.

## Building a UF2

To produce a `.uf2` file for drag-and-drop flashing:

```sh
./build.sh
```

The output is `target/thumbv6m-none-eabi/release/sensor-watch.uf2`. The script
builds the release firmware, extracts the raw binary with `rust-objcopy`, and
converts it to UF2 using the `uf2tool` binary (run on the host target).

## Linting

```sh
cargo clippy --target thumbv6m-none-eabi
cargo clippy -p sensor-watch-core -- -D warnings
```

The firmware clippy job is informational because the full C-reference HAL API
and many ported faces carry intentional dead-code and pedantic style lints. The
`core` clippy job is the strict gate (`-D warnings`). This checkout's host test
attempt is currently blocked by the existing `vec!` macro import error in
`core/src/uf2.rs`, before a complete warning report is produced.

## Formatting

```sh
cargo fmt --check
```

## CI

A GitHub Actions workflow (`.github/workflows/ci.yml`) runs on every push:

- Build (thumbv6m)
- Clippy (`-D warnings`)
- Host tests
- Format check

---

## Project structure

```
sensor-watch-rs/
|-- Cargo.toml          # workspace + firmware package
|-- core/               # pure logic, host-testable
|-- src/
|   |-- main.rs         # entry point
|   |-- panic.rs        # panic handler
|   |-- watch/          # hardware abstraction layer
|   `-- movement/       # watchface framework + faces
|-- docs/               # this documentation
`-- .github/workflows/  # CI
```

---

## How to add a new watch face

1. Create a new file `src/movement/<name>.rs`.
2. Define a state struct with a `new_static()` const constructor.
3. Implement the `WatchFace` trait:
   - `setup()` - one-time init
   - `activate()` - prepare to go on-screen
   - `loop_()` - react to events, update the display
   - `resign()` - prepare to go off-screen
   - `wants_background_task()` - optional
   - `advise()` - optional, called once per minute for all faces
4. Add the module to `src/movement/mod.rs`.
5. Add a `static` instance and register it in `app_setup()`.
6. If you add a face, bump `MOVEMENT_NUM_FACES` in `types.rs` to match.

### Example skeleton

```rust
use crate::movement::types::{Event, Settings, WatchFace};

pub struct MyFace {
    // state fields
}

impl MyFace {
    pub const fn new_static() -> Self {
        MyFace { /* init */ }
    }
}

impl WatchFace for MyFace {
    fn setup(&mut self, _settings: &Settings, _index: usize) {}
    fn activate(&mut self, _settings: &Settings) {}
    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            // handle events
            _ => {}
        }
    }
    fn resign(&mut self, _settings: &mut Settings) {}
}
```

---

## Design rules to follow

1. **No heap.** Everything is a `static` instance or a fixed-size stack buffer.
   Do not allocate.
2. **No polling.** If a face needs periodic work, schedule an RTC wakeup. Never
   keep the CPU awake.
3. **React and sleep.** A face's `loop_` reacts to one event and returns. It
   never loops.
4. **Release peripherals.** If a face enables a peripheral, it's auto-released
   after the reaction. Don't leave anything on.
5. **Persist settings.** If a face changes a setting, it's auto-saved. Don't
   write to flash during normal operation.
6. **Bounded waits.** Never use an unbounded `while !flag {}`. Use
   `wait_until()`.
7. **Document.** Every public item should have a doc comment explaining what and
   why.
8. **Flash alignment.** Any struct persisted to flash should be
   `#[repr(C, align(4))]`.
9. **Flash writes from RAM.** Flash write/erase routines must be marked
   `#[unsafe(link_section = ".ramfunc")]` to avoid the read-while-write stall.

---

## Testing philosophy

The firmware itself can't run unit tests (it's bare-metal). So:

- **Pure logic** lives in the `core` crate, which is host-testable.
- **Hardware drivers** are verified by review and by the CI build.
- **Faces** are verified by review and by the CI build.

When you add logic that has no hardware dependency, put it in `core` so it can be
tested.
