# Firmware Studio

A **GUI companion app** for the Sensor-Watch firmware. This is the end-goal
product: an editor, debugger, and assembler that produces the final `.uf2`
firmware file.

Built with **egui/eframe** (pure Rust, cross-platform GUI).

## Panels

- **Dashboard** — project overview and health (target, flash/RAM, face count,
  last build)
- **Watch Faces** — lists all 111 faces registered in the firmware (scanned
  from `src/movement/mod.rs`)
- **Build** — assembles the firmware into a `.uf2` (runs `cargo build`, extracts
  the raw binary with `rust-objcopy`, and converts to UF2 with the
  `sensor-watch-core` encoder)
- **Flash** — copies the built `.uf2` to the watch's USB drive (bootloader mode)
- **Settings** — configure the app and the watch

## Building

```sh
cargo build --release
```

The binary is `target/release/sensor-watch-studio`.

## Reusing the firmware logic

The app depends on `sensor-watch-core` (from `../sensor-watch-rs/core`), which
provides the pure logic (UF2 encoding, date math, settings bit-packing) that is
host-testable and directly reusable by the app. The Build panel invokes the
firmware's own `cargo build` and uses the core crate's `convert_to_uf2` to
produce the final file.

## License

MIT OR Apache-2.0.
