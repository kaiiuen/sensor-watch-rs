# Firmware Studio

A **GUI companion app** for the Sensor-Watch firmware. This is the end-goal
product: an editor, debugger, and assembler that produces the final `.uf2`
firmware file.

Built with **egui/eframe** (pure Rust, cross-platform GUI).

## Panels

- **Dashboard** — project overview and health: target board selection
  (Green/Red-Lite/Blue/Pro), flash/RAM estimates, last build time + build count,
  current OS date/time, NTP time fetch (auto-fetches from Cloudflare on launch),
  custom NTP server management, clock calibration, drift calibration, fuzz
  testing, and a server list.
- **Watch Faces** — lists all faces registered in the firmware (scanned from
  `src/movement/mod.rs`) with a **search box**, preset management
  (create/rename/delete presets, add/reorder/remove faces, spreadsheet-style
  grids), and the **Watch Settings** panel (clock mode, sound/buzzer, LED,
  power/motion, timezone with a country dropdown).
- **Editor** — create, edit, or delete watch faces from templates.
- **Simulator** — 1:1 F-91W replica (SVG) with clickable button hotspots, a
  **date/time controller**, and face cycling through the active preset. Faces
  are **fully interactive** via the `face_sim` engine: the clock ticks live,
  the stopwatch/timer/counter run, the alarm toggles, and the diagnostics face
  is navigable (including a power-on uptime stat). Text renders with the
  firmware's real 7-segment character set.
- **Build** — assembles the firmware into a `.uf2` with a dedicated **build log**.
- **Flash** — copies the built `.uf2` to the watch's USB drive, with **watch
  detection** and a dedicated **flash log**.
- **Debug** — background activity log with Copy All / Export / Clear.
- **Bugs** — dedicated error/warning log for troubleshooting.
- **Settings** — language, theme, text size (small/normal/big), app resource
  usage (app-only, adjustable update rate), settings save/export/import,
  source export, integrity (SHA-256 + release checksum verification), credits
  with code statistics, and license.

## Footer

- **Watch stats** — number of selected faces, estimated flash/RAM/compiled size
- **Window size** — current window dimensions
- **Error counter** — jumps to the Bugs tab
- **Status** — last status message

## Building

```sh
cargo build --release
```

The binary is `target/release/sensor-watch-studio`. It is fully self-contained
(the watch SVG is embedded), so it can be copied anywhere and run.

## Reusing the firmware logic

The app depends on `sensor-watch-core` (from `../sensor-watch-rs/core`), which
provides the pure logic (UF2 encoding, date math, settings bit-packing, SECDED
ECC) that is host-testable and directly reusable by the app. The Build panel
invokes the firmware's own `cargo build` and uses the core crate's
`convert_to_uf2` to produce the final file.

## Dependencies

- `eframe` / `egui` — GUI framework
- `resvg` / `usvg` — SVG rendering for the watch face
- `serde` / `serde_json` — settings save/export/import
- `arboard` — system clipboard
- `sysinfo` — app resource usage
- `ureq` — HTTP client for release checksum verification
- `webbrowser` — open the GitHub repo from the title

## Source layout

- `main.rs` — the app shell and all panels
- `face_sim.rs` — the stateful watch-face simulation engine
- `watch_display.rs` — renders faces to the SVG using the firmware character set
- `watch_sim.rs` — the F-91W clock/light/CASIO logic and live time accessor
- `build.rs` — firmware build → UF2, path resolution
- `faces.rs` — discovers faces from the firmware `mod.rs`
- `editor.rs` — face templates + read/write/delete
- `presets.rs` — preset manager
- `i18n.rs` — language (English)
- `theme.rs` — Light/Dark/Auto
- `debug.rs` — ring-buffer log
- `ntp.rs` — NTP time client
- `settings.rs` — settings save/export/import
- `persist.rs` — internal settings persistence (exe-adjacent file)
- `integrity.rs` — SHA-256 hashing and release checksum verification
- `sysstats.rs` — app resource usage
- `watch_config.rs` — watch configuration (mirrors the firmware Settings register)
- `drift.rs` — drift calibration
- `fuzz.rs` — face-engine fuzz testing

## License

MIT OR Apache-2.0.
