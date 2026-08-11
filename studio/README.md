# Firmware Studio

A **GUI companion app** for the Sensor-Watch firmware. This is the end-goal
product: an editor, debugger, simulator, and assembler that produces the final
`.uf2` firmware file and flashes it to the watch.

Built with **egui/eframe** (pure Rust, cross-platform GUI).

## Panels

- **Dashboard** - project overview and health: target board selection
  (Green/Red-Lite/Blue/Pro) with a board-info description beside the selector,
  flash/RAM estimates, last build time + build count, current OS date/time, NTP
  time fetch (auto-fetches from Cloudflare on launch), custom NTP server
  management (add/edit/delete), a server list, clock calibration, drift
  calibration, and fuzz testing. All sections are collapsible and default to
  expanded.
- **Watch Faces** - lists all faces registered in the firmware (scanned from
  `src/movement/mod.rs`) with a **search box**, **category filter**, preset
  management (create/rename/delete presets, add/reorder/remove faces via
  drag-and-drop, spreadsheet-style grids), and the **Watch Settings** panel
  (clock mode, sound/buzzer, LED/backlight, power/motion, timezone with a
  country dropdown). Right-clicking a face opens a **context menu** with
  preview / view code / test-before-adding actions. The catalog and active
  preset are stacked on the left and watch settings on the right; panel sizes
  are persisted and reset proportionally when the window is resized.
- **Editor** - a self-IDE for creating, editing, or deleting watch faces from
  templates, with a collapsible "How to make a watch face" guide and a
  **description** field that shows up in the catalog.
- **Simulator** - 1:1 F-91W replica (SVG) with clickable button hotspots, a
  **date/time controller**, and face cycling through the active preset. Faces
  are **fully interactive** via the `face_sim` engine: the clock ticks live,
  the stopwatch/timer/counter run, the alarm toggles, and the diagnostics face
  is navigable (including a power-on uptime stat). Text renders with the
  firmware's real 7-segment character set. Shows both the sim's face counter
  and the engine's actual loaded face for catching face-switching bugs. With
  the opt-in `real-faces` feature, the simulator runs the **real firmware
  faces** through the `real_face.rs` host seam (see below) instead of the
  hand-written engine.
- **Build & Flash** - combined panel: select the target board, build the
  firmware into a `.uf2` (with estimated compile/flash times), then flash it to
  the watch. The app **auto-detects** the watch's USB drive and auto-selects
  the board from its `INFO_UF2.TXT`, copies the `.uf2`, and auto-fetches NTP
  time for sync. Has a combined build & flash log.
- **Calibration** - clock calibration (generates a `settime` command for the
  next minute boundary), a **beep-on-minute-rollover** helper, and drift
  calibration (parts-per-million).
- **Modules** - register custom hardware modules for modded boards (e.g. a BLE
  board instead of the accelerometer). Each module targets a HAL file in
  `src/watch/`; modules are persisted and can be enabled/disabled/removed.
- **Debug** - background activity log with Copy All / Export / Clear. Logs
  auto-scroll to the bottom and honor a configurable line limit.
- **Bugs** - dedicated error/warning log, plus a **Generate bug report** button
  that copies a structured report (app state + recent errors/activity) to the
  clipboard.
- **Tutorials** - a beginner-friendly, plain-language guide to making watch
  faces: what a face is, how the buttons work, and how to build your first one.
- **Wiki** - a built-in reference browser for project concepts, with search,
  navigation history, and **Browse repos** buttons that open the upstream
  Sensor-Watch and author's repos in the browser.
- **Settings** - language (English, 简体中文, 繁體中文), theme, text size
  (small/normal/big), configurable **log line limit**, app resource usage
  (app-only, adjustable update rate), settings save/export/import, source
  export, integrity (SHA-256 + release checksum verification), credits with
  code statistics, and license.

## Terminal

A collapsible **terminal** sits above the footer and persists across all tabs.
It accepts commands for power users and for testing the app:

```
help, status, faces, board, build, flash, fuzz, time, clear,
modules, errors, bugreport, sim <a|b|c>, theme, lang
```

## Footer

- **Watch stats** - number of selected faces, estimated flash/RAM/compiled size
- **Window size** - current window dimensions
- **Error counter** - jumps to the Bugs tab
- **Status** - last status message

## Building

```sh
cargo build --release
```

The binary is `target/release/sensor-watch-studio`. It is fully self-contained
(the watch SVG is embedded), so it can be copied anywhere and run. The app
launches at a 480p (640x480) default window size and is resizable.

## Reusing the firmware logic

The app depends on `sensor-watch-core` (from `../sensor-watch-rs/core`), which
provides the pure logic (UF2 encoding, date math, settings bit-packing, SECDED
ECC) that is host-testable and directly reusable by the app. The Build panel
invokes the firmware's own `cargo build` and uses the core crate's
`convert_to_uf2` to produce the final file.

The Simulator can also run the **real firmware faces** through a host seam:
`real_face.rs` drives the firmware's own `WatchFace` code against a mock HAL,
so the rendered digits come from the same code the firmware runs. This is an
**opt-in** `real-faces` feature (off by default) because it requires the
firmware host lib to compile as a host dependency; with it off, the Simulator
falls back to the hand-written `face_sim` engine.

## Dependencies

- `eframe` / `egui` - GUI framework
- `resvg` / `usvg` - SVG rendering for the watch face
- `serde` / `serde_json` - settings save/export/import
- `arboard` - system clipboard
- `sysinfo` - app resource usage
- `ureq` - HTTP client for release checksum verification
- `webbrowser` - open the GitHub repo from the title

## Source layout

- `main.rs` - the app shell and all panels
- `face_sim.rs` - the stateful watch-face simulation engine
- `watch_display.rs` - renders faces to the SVG using the firmware character set
- `watch_sim.rs` - the F-91W clock/light/CASIO logic and live time accessor
- `build.rs` - firmware build → UF2, path resolution
- `faces.rs` - discovers faces from the firmware `mod.rs`
- `editor.rs` - face templates + read/write/delete
- `presets.rs` - preset manager
- `i18n.rs` - language (English, Simplified/Traditional Chinese)
- `theme.rs` - Light/Dark/Auto
- `fonts.rs` - loads a system CJK font so Chinese renders
- `debug.rs` - ring-buffer log
- `ntp.rs` - NTP time client
- `settings.rs` - settings save/export/import
- `persist.rs` - internal settings persistence (exe-adjacent file)
- `integrity.rs` - SHA-256 hashing and release checksum verification
- `sysstats.rs` - app resource usage
- `watch_config.rs` - watch configuration (mirrors the firmware Settings register)
- `modules.rs` - custom hardware module registry
- `drift.rs` - drift calibration
- `fuzz.rs` - face-engine fuzz testing
- `wiki.rs` - the built-in reference wiki
- `real_face.rs` - host seam that runs real firmware faces (behind the opt-in
  `real-faces` feature)

## License

MIT OR Apache-2.0.
