# Firmware Studio

A **GUI companion app** for the Sensor-Watch firmware. This is the end-goal
product: an editor, debugger, simulator, and assembler. Studio generates
configured UF2 artifacts for supported stock board, revision, LCD, component,
and face inputs. Unsupported or incomplete configurations fail closed so a
stock artifact is never misrepresented as configured firmware.

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
  preset are stacked on the left and watch settings on the right, panel sizes
  are persisted and reset proportionally when the window is resized.
- **Explorer** - a bounded browser for the app-local data root and active mutable project. It never exposes package/version resources, bundled templates, generated outputs, or secrets.
- **Notepad** - Markdown notes kept under `<app data root>/notes`, categorized as Project, Hardware, Firmware, Build, Testing, Ideas, or Archive. Notes support bounded source editing, optional plain preview, search/category filters, tabs, atomic saves, external-change conflict detection, and confirmation-gated deletion. Note paths are policy-checked and never leave the app-local root.
- **Compare** - a read-only source comparison view opened from the Editor,
  Watch Faces, or Explorer. It shows immutable stock/modified documents with
  path, role, face identity, and SHA-256 headers, bounded line diff hunks with
  added/removed/changed markers, previous/next navigation, side-by-side or
  top-bottom layout, and optional synchronized scrolling. Reads use the same
  filesystem policy as Explorer; secrets, binaries, oversized files, links,
  reparse points, unsafe roots, and missing sources are rejected. Comparison
  never saves, applies, or mutates either document. Face comparisons pair only
  matching `faces::face_identity` IDs by default; unrelated files require the
  explicit opt-in.
- **Editor** - a self-IDE for creating, editing, or deleting watch faces from
  templates, with a collapsible "How to make a watch face" guide and a
  **description** field that shows up in the catalog. The beginner-safe
  **Blocks** mode uses the small visual starter API. **Advanced** mode exposes
  the full Rust source and is intended for experienced users who understand the
  firmware API, build constraints, and hardware impact. Normal mode is the
  recommended path for beginners, Advanced mode does not make source changes
  safe or physically validated.
- **Simulator** - F-91W SVG display replica with clickable button hotspots, a
  **date/time controller**, and face cycling through the active preset. The
  current face render path is shown prominently in the panel. With the default
  `real-faces` feature, all 111 faces execute the actual firmware face source
  files through translated host movement and HAL seams with `MockHw`. With
  `real-faces` disabled, all 111 faces use `face_sim`. This is not full ARM
  firmware simulation or hardware simulation. MMIO, interrupts, sensors, power,
  RTC oscillator
  accuracy, peripheral electrical behavior, and some scheduling are modeled or
  stubbed and can diverge from a physical watch. The SVG and firmware character
  set provide a display preview, not physical hardware validation.
- **Build & Flash** - combined panel: review the target board and component
  profile, then request a firmware build. Supported stock configurations emit
  validated generated inputs and artifacts. Unsupported or incomplete inputs
  fail closed with the reason shown in the profile panel. Build failures
  include bounded Cargo and tool diagnostics in the panel, status, terminal,
  error log, and a safe `build.log` under the resolved output path. The panel
  still documents the intended USB-drive/`INFO_UF2.TXT` workflow.
- **Calibration** - guided clock calibration (generates a `settime` command for
  the next minute boundary), a **beep-on-minute-rollover** helper, and guided
  manual RTC drift calibration in parts-per-million, and optional temperature
  compensation settings. Manual ppm correction and temperature compensation
  are separate. The hardware path requires UART Jig mode, and the default
  simulated path does not change a watch.
- **Shell Access** - explicit **Simulated** mode (the default, using the existing
  in-app watch model) or **UART Jig** mode. UART mode discovers host serial
  ports, opens a selected port at **9600 8-N-1**, and exchanges CR/LF-framed
  shell commands with bounded read/write timeouts. It is for the debug pads
  (A2 TX, A3 RX, GND), not the watch's UF2 USB port, USB CDC is not assumed.
  A missing port can mean the jig is disconnected, off, miswired, or not passed
  through to the host. It does not prove that the board lacks UART.
- **Modules** - register custom hardware modules for modded boards (e.g. a BLE
  board instead of the accelerometer). Each module targets a HAL file in
  `src/watch/`, modules are persisted and can be enabled/disabled/removed.
  Component profiles are implemented in Studio as persisted configuration and
  planning-estimate UI, and supported stock profiles now generate validated
  firmware build flags, pin mappings, and provenance. Unsupported custom
  combinations remain unavailable. Thermistor and OPT3001 readings still
  require matching hardware.
  simulator diagnostics do not create sensor measurements.
- **Diagnostics / Probe-Test** - offline simulator and shell diagnostics. A
  connected UART is shown separately, but the diagnostic report does not query
  physical hardware automatically. Use the explicit UART Jig path for shell
  observations and an SWD probe for silicon-level debugging.
- **Debug Output** - read-only background activity log with Copy All / Export / Clear. It remains visible in Normal mode. Logs
  auto-scroll to the bottom and honor a configurable line limit.
- **Bugs** - dedicated error/warning log, plus a **Generate bug report** button
  that copies a structured report (app state + recent errors/activity) to the
  clipboard.
- **Tutorials** - a beginner-friendly, plain-language guide to making watch
  faces: what a face is, how the buttons work, and how to build your first one.
- **Wiki** - a built-in reference browser for project concepts, with search,
  navigation history, and **Browse repos** buttons that open the upstream
  Sensor-Watch and author's repos in the browser.
- **Settings** - language (English, Simplified Chinese, Traditional Chinese), theme, text size
  (small/normal/big), configurable **log line limit**, app resource usage
  (app-only, adjustable update rate), settings save/export/import, source
  export, integrity (SHA-256 digest/checksum checks only, not authenticity), community credits
  (see [`docs/CREDITS.md`](../docs/CREDITS.md)), code statistics, and license.

## Studio build input contract

The profile-to-build path is available for supported stock combinations. Studio
and firmware validate all of the following inputs before any build side effect:

1. Active preset identity plus the ordered face/source inputs.
2. Target board identity, revision, and board-specific runtime settings.
3. Component-to-firmware feature/module selections.
4. Concrete pin, bus, address, power, and ownership mappings for every selected
   component. Enabling SPI or I2C alone is not a pin mapping, and a thermistor
   cannot be mapped from the component name alone.
5. A generated-input provenance and validation record tied to the exact firmware
   build, so the resulting UF2 can be identified as configured rather than stock.

Unsupported or incomplete profiles remain planning-only. Studio shows the
specific validation issue and rejects the build before filesystem, Cargo, or
UF2 side effects. No pin assignments are inferred for unsupported combinations.

## Advanced and Probe/Test safety

Normal mode presents Dashboard, Explorer, Watch Faces, Notepad, Editor, Simulator, Build & Flash, Settings, and read-only Debug Output. Advanced mode adds Calibration, Modules, Diagnostics, Bugs, and detailed artifact tools. Developer mode is a separate explicit opt-in for Shell, Probe / Test, Terminal, Master Clock, and raw experimental tools. These presentation modes never weaken a hard safeguard.

Studio's normal mode is beginner-safe in the sense that it uses the simulator,
Blocks editor, and normal UF2 workflow. It does not claim that simulated output
is a physical test. Advanced mode is for experienced users and includes direct
Rust editing and access to hardware-oriented workflows, review every command
and target before using it.

The watch USB bootloader drive exposes UF2/file-transfer information only. It
can be used to copy a `.uf2` and inspect bootloader files, but it is not a
serial console and does not replace the UART jig on A2/A3/GND.

The UART shell separates commands into:

- Read-only: `help`, `time`, `drift` without a value, `panic`, `events`, and
  `optical` when available.
- Mutating: `settime YYMMDDHHMMSS`, `drift N`, and `events clear`.

Check the wiring and target before mutating the RTC, drift correction, or event
ring. Power down while changing wires, use a 3.3 V-compatible adapter, cross
TX and RX, and connect GND. Never apply 5 V UART signaling to the watch. Do not
use a write timeout or missing port as evidence that the watch rejected a
command or lacks UART.

Probe/Test results must use distinct labels:

- **PASS** - the check ran and passed its acceptance condition.
- **FAIL** - the check ran and did not pass.
- **NOT AVAILABLE** - required hardware or transport is unavailable.
- **NOT TESTED** - no check was run and no conclusion is available.

A simulated PASS is a software result only. It must not be reported as a
physical hardware PASS. A disconnected or unavailable UART is NOT AVAILABLE,
not FAIL, an unrun test is NOT TESTED.

## Terminal

A collapsible **terminal** sits above the footer and persists across all tabs.
It accepts commands for power users and for testing the app:

```
help, status, faces, board, build, flash, fuzz, time, clear,
modules, errors, bugreport, sim <a|b|c>, theme, lang

The firmware UART shell is separate from this terminal. Its current commands
are `time`, `settime YYMMDDHHMMSS`, `drift N`, `optical`, `panic`, `events`,
`events clear`, and `help`.
```

## Footer

- **Watch stats** - number of selected faces, estimated flash/RAM/compiled size
- **Window size** - current window dimensions
- **Error counter** - jumps to the Bugs tab
- **Status** - last status message

## Packaged storage contract

A packaged launch is identified by `sensor-watch-package.json`, whether Studio is
started by the launcher or directly from `versions/<version>`. Its mutable root
is exactly `<package root>/data`: settings, runtime preferences, restore
points, the mutable project, notes, logs and exports, update state, startup markers,
and launcher state stay below that directory. Notepad uses `<package root>/data/notes`
with category metadata for each supported category. Firmware build output is separate at
`<package root>/data/sensor-watch-studio-artifacts/<board>/<revision>/<profile>/latest`.
Recovery generations are under that artifact root. Immutable version directories
and bundled templates are never used for mutable writes. Packaged builds copy and
validate the bundled template into the mutable project first, then build only
from that active project.

The launcher forwards the package root, the same package-local data root, version, attempt, and portable context explicitly. A package-root write failure is reported as an
actionable error; Studio does not silently fall back to Windows AppData,
Documents, or legacy settings. Developer checkouts may continue to use the
normal user-scoped defaults. Packaged mode never accepts a custom data root. If the package root or its data directory is protected or unwritable, move or extract the application to a writable folder.

## Building and all-in-one CLI

Build the GUI with:

```sh
cargo build --release
```

The same binary also provides host-side configured-build, UF2, recovery, and
probe-flash commands. `configured-build` is the headless Studio path: it accepts
only the supported stock board/revision/profile combinations, original LCD,
an active preset with ordered faces, and a build output root. The `--output` value
is authoritative for that invocation. Output resolves to
`<output>/<board>/<revision>/<profile>/latest`. It constructs the same
`FirmwareInputRequest` used by the GUI and verifies generated-input
provenance before returning success. The separate `build` command is explicitly
the unconfigured stock firmware path. Use `help` to see the complete command
surface:

```sh
cargo run -p sensor-watch-studio -- help
cargo run -p sensor-watch-studio -- configured-build \\
  --board Green --revision OSO-SWAT-A1-05 --profile Green --lcd original \\
  --preset "Stock Casio" --faces SIMPLE_CLOCK,ALARM \\
  --output target/configured-green  # prints the resolved latest and artifact paths
cargo run -p sensor-watch-studio -- build  # stock, unconfigured firmware
cargo run -p sensor-watch-tools -- package-studio
cargo run -p sensor-watch-tools -- package-studio --output target/releases/sensor-watch-studio-0.1.0.zip
cargo run -p sensor-watch-studio -- verify path/to/sensor-watch.uf2
cargo run -p sensor-watch-studio -- backup input.uf2 recovery/known-good.uf2
cargo run -p sensor-watch-studio -- rollback input.uf2 staged.uf2 TRUSTED_SHA256
cargo run -p sensor-watch-studio -- report input.uf2 TRUSTED_SHA256
cargo run -p sensor-watch-studio -- flash [ELF]
```

These commands operate on the host. The stock `sensor-watch-tools -- build`
path remains the command for producing an unconfigured firmware UF2. Studio does
not apply its selections to that artifact. `flash` requires a
probe-rs-compatible SWD probe, the CLI does not add USB CDC, modify the UF2
bootloader, or provide device-side rollback. With no command, Firmware Studio
starts its GUI.

The binary is `target/release/sensor-watch-studio`. A release executable by
itself is not a full distribution. Folder-based packages must place
`sensor-watch-package.json` beside the package root and identify the launcher,
versioned app directory, resources, templates, firmware project, and optional
tools/targets and the optional `master_clock` capability. Studio reports
**Packaged mode** only after validating that manifest. Missing project sources or
tools are reported as unavailable. The Master Clock action is Advanced-only,
on-demand, package-local, hash-validated, and never launched at startup. It
warns that NTP/geolocation are external network activity. Windows time changes only through the tool's own
explicit control. The package builder does not bundle the unlicensed/untracked
Master Clock source. All mutable settings and data remain under `<package root>/data`.

A binary copied from a developer checkout does not silently use that checkout.
To opt into checkout paths for local development, set
`SENSOR_WATCH_STUDIO_DEVELOPER_MODE=1` before launch. Without that explicit
setting, Studio reports unavailable package resources. In-place updates,
downloads, and self-update are intentionally outside this foundation.

The app launches at a 480p (640x480) default window size and is resizable.

## Reusing the firmware logic

The app depends on `sensor-watch-core` (from `../core`), which provides the
pure logic (UF2 encoding, date math, settings bit-packing, SECDED ECC, transfer
validation, and optical protocol validation) that is host-testable and directly
reusable by the app. The Build panel validates its selected preset/faces, board,
revision, LCD, and component profile, then passes generated inputs into the
firmware build. It invokes the firmware's own `cargo build` and uses the core
crate's
`convert_to_uf2` to produce the final file. Unsupported combinations fail
closed.

The Simulator's default `real-faces` mode executes all 111 faces from the
actual firmware face source files through translated host movement and HAL seams
with `MockHw`. If `real-faces` is disabled, the existing fallback behavior
remains in place and all 111 faces use `face_sim`.

This is not full ARM firmware simulation or hardware simulation. MMIO,
interrupts, sensors, power, RTC oscillator accuracy, peripheral electrical
behavior, and some scheduling are modeled or stubbed and can diverge from a
physical watch. The display preview and host seam coverage do not constitute
physical hardware validation. CI checks the fallback configuration with
`cargo test -p sensor-watch-studio --no-default-features`. The latest validated
workspace run passed 365 host tests: 121 firmware host-seam, 69 core, 145
Studio, and 30 tools. The ARM release package build is a separate build check.
None of these results represent on-silicon validation.

## Dependencies

- `eframe` / `egui` - GUI framework
- `resvg` / `usvg` - SVG rendering for the watch face
- `serde` / `serde_json` - settings save/export/import
- `arboard` - system clipboard
- `sysinfo` - app resource usage
- `ureq` - HTTP client for release checksum/integrity checks (not authenticity)
- `serialport` - cross-platform UART-jig port discovery and I/O
- `webbrowser` - open the GitHub repo from the title

## Source layout

- `main.rs` - the app shell and all panels
- `face_sim.rs` - the stateful watch-face simulation engine
- `watch_display.rs` - renders faces to the SVG using the firmware character set
- `watch_sim.rs` - the F-91W clock/light/CASIO logic and live time accessor
- `build.rs` - firmware build -> UF2, path resolution
- `faces.rs` - discovers faces from the firmware `mod.rs`
- `editor.rs` - face templates + read/write/delete
- `file_browser.rs` - bounded Explorer UI, navigation history, details view, and editor-open bridge
- `fs_policy.rs` - centralized canonical path, root, size, link, and mutation policy
- `presets.rs` - preset manager
- `i18n.rs` - language (English, Simplified/Traditional Chinese)
- `theme.rs` - Light/Dark/Auto
- `fonts.rs` - loads a system CJK font so Chinese renders
- `debug.rs` - ring-buffer log
- `ntp.rs` - NTP time client
- `settings.rs` - settings save/export/import
- `persist.rs` - internal settings persistence (exe-adjacent file)
- `integrity.rs` - SHA-256 integrity hashing/checks. Release authenticity requires separately trusted public-key signature verification
- `sysstats.rs` - app resource usage
- `watch_config.rs` - watch configuration (mirrors the firmware Settings register)
- `components.rs` - persisted component/build profiles and planning estimates
- `modules.rs` - custom hardware module registry
- `drift.rs` - drift calibration
- `fuzz.rs` - face-engine fuzz testing
- `wiki.rs` - the built-in reference wiki
- `real_face.rs` - host seam that runs real firmware faces (behind Studio's
  default-enabled `real-faces` feature)
- `transport.rs` - simulated/UART-jig transport selection, serial discovery,
  line framing, timeout handling, and host tests

### Explorer boundaries

Studio Explorer starts at the active project when that project is available.
It also exposes the bounded app-data root, with Back, Forward, Up, Refresh,
breadcrumbs, and a Name/Type/Size/Modified details view. Directory double-click
opens the directory; file double-click uses the existing editor bridge.

Package, version, template, resource, and generated roots are shown as
read-only or unavailable where applicable; policy-reserved roots remain
unavailable. New, rename, and delete operations remain confirmation-gated and
are submitted only through `fs_policy.rs`.
Search remains bounded and continues to exclude sensitive files and unsafe
links. Windows Explorer is launched only with a validated canonical path in an
argument vector; Studio does not provide unrestricted external root access.

## License

MIT OR Apache-2.0.
