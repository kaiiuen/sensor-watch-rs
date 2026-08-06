# Firmware Studio

A **GUI companion app** for the Sensor-Watch firmware. This is the end-goal
product: an editor, debugger, and assembler that produces the final `.uf2`
firmware file.

Built with **egui/eframe** (pure Rust, cross-platform GUI).

## Panels

- **Dashboard** — project overview and health (target, flash/RAM, face count,
  last build, current OS date/time, NTP time fetch)
- **Watch Faces** — lists all faces registered in the firmware (scanned from
  `src/movement/mod.rs`) with a **search box**, plus preset management
  (create/rename/delete presets, add/reorder/remove faces, spreadsheet-style
  grids)
- **Editor** — create, edit, or delete watch faces from templates
- **Simulator** — 1:1 F-91W replica (SVG) with clickable button hotspots, a
  **date/time controller** (year/month/day/hour/minute/weekday), and face
  cycling through the active preset
- **Build** — assembles the firmware into a `.uf2` (runs `cargo build`, extracts
  the raw binary with `rust-objcopy`, and converts to UF2 with the
  `sensor-watch-core` encoder)
- **Flash** — copies the built `.uf2` to the watch's USB drive (bootloader mode)
- **Debug** — background activity log (time + message columns)
- **Settings** — configure the app (language, theme), the watch (clock mode,
  sound/buzzer, LED/backlight, power/motion), app resource usage, settings
  save/export/import, source export, and credits with links to the original repos

## Footer

- **Watch stats** — number of selected faces, estimated flash/RAM/compiled size
  (based on the selected preset faces)
- **Status** — last status message

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

## Dependencies

- `eframe` / `egui` — GUI framework
- `resvg` / `usvg` — SVG rendering for the watch face
- `serde` / `serde_json` — settings save/export/import
- `arboard` — system clipboard
- `sysinfo` — app/system resource usage

## License

MIT OR Apache-2.0.
