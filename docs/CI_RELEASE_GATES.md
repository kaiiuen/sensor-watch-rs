# CI and release gates

The required checks in `.github/workflows/ci.yml` cover software-completable
behavior. A passing check means the named source, host seam, target compilation,
or artifact contract was exercised; it does not claim that a physical watch was
connected or electrically validated.

## Blocking gates

- `fmt`: `cargo fmt --check`.
- `core-tests`: core host tests and `cargo clippy -p sensor-watch-core -- -D warnings`.
- `studio`: Studio builds and tests with both default `real-faces` and
  `--no-default-features`.
- `firmware-host-features`: hostmock tests for the runtime UART/identity seam,
  optical, shell authorization, combined optical/USB/shell, and minimal-USB
  contracts. These names describe software contracts; they do not imply a
  connected UART, sensor, or USB device.
- `firmware-arm`: ARM compile checks for the default, optical, Pro optical
  receive, USB CDC, RTT logging, identity/UART baseline, and minimal-USB
  profiles.
- `arm-release` and `minimal-usb`: release linking, flash-size budgets, UF2
  generation, and UF2/manifest/sidecar verification.
- `board-configured`: documented board mapping/configuration tests plus the
  ignored configured Green and Red-Lite isolated builds. Generated-input
  provenance and manifest checks are explicit gates.
- `release-package`: the decomposed Studio ZIP command and package tests,
  including deterministic layout, complete `PACKAGE-MANIFEST.json`, launcher
  layout, and Master Clock omission/inclusion and metadata behavior.
- `launcher-update`: launcher startup/recovery/path/hash behavior and signed
  desktop-update policy tests.
- `windows-tooling`: Windows path-policy and UF2 tooling tests, plus launcher
  and update tests on Windows.
- `audit`: dependency vulnerability audit.

The current face contract is checked by Studio tests: 111 registered/catalog
faces, and the default real-face path executes all 111. The `face_sim`
fallback remains available in the no-default-features configuration. If that
final count changes, update the corresponding tests and this statement together.

## Non-gating hardware validation

Physical flashing, boot/recovery, current draw, RTC drift, buttons, display,
LED polarity, buzzer, sensors, I2C, UART wiring, and USB bootloader behavior
remain manual procedures in `docs/TESTING.md` and `docs/HARDWARE_ACCESS.md`.
The disabled `hardware-tests-not-gating` workflow job is a visible reminder of
that boundary and is deliberately not a required check.

## Limitations

The CI package job creates a local-development unsigned ZIP. It verifies layout,
content hashes, and optional-tool metadata, but it cannot create production
release signatures or establish publisher authenticity. Master Clock remains
optional and is not bundled unless an explicit, licensed, provenance-backed
input is supplied. The configured board test depends on the ARM target and
`cargo-binutils`/`rust-objcopy`; environments without those prerequisites will
report the build as unavailable rather than silently treating it as a pass.
