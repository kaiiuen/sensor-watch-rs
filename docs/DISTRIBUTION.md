# Studio folder distribution foundation

This repository defines a bounded, folder-based Windows distribution contract
for Firmware Studio. It now includes an offline/self-update foundation, but it
does not implement network downloads or in-place replacement of the running
executable.

## Package manifest

A package root contains `sensor-watch-package.json`. The manifest is validated
before Studio enters packaged mode:

```json
{
  "schema_version": 1,
  "current_version": {"version": "1.2.3", "installed_at": "2026-01-01T00:00:00Z"},
  "previous_version": {"version": "1.2.2"},
  "launcher_executable": "launcher/sensor-watch-studio.exe",
  "app_directory": "app/1.2.3",
  "resources_directory": "resources",
  "templates_directory": "templates",
  "firmware_project_directory": "firmware",
  "tools_directory": "tools",
  "targets_directory": "targets",
  "user_data_directory": "user-data"
}
```

All manifest paths are package-relative and may not be absolute or contain
`..`. The launcher path must resolve to the executable that is running. Studio
searches only the executable directory and its ancestors for this manifest.
The package root, launcher, version metadata, and each capability are exposed
through the typed `distribution` contract in `studio/src/distribution.rs`.

`resources`, `templates`, and `firmware_project` are reported independently.
`tools` and `targets` are optional paths but are still reported as capabilities
when present. A package is never described as self-contained when any required
project/resource capability or optional tool/target bundle is absent.

## Runtime modes and mutable data

- **Packaged mode** uses only paths resolved from the validated manifest.
- **Developer checkout mode** is available only when
  `SENSOR_WATCH_STUDIO_DEVELOPER_MODE=1` is explicitly set. It resolves the
  compiled developer workspace and is labeled separately in the GUI.
- With no manifest and no explicit developer mode, project resources are
  unavailable; Studio does not silently use a checkout.
- Settings, presets, logs, restore points, and generated user data stay in the
  platform user-data root and are not written into the package root.

The footer displays the mode, current package version when available, and
whether the distribution is complete or partial. Missing resources are shown
as capability warnings rather than being inferred from the executable.

## Launcher startup handshake and recovery

The standalone launcher passes an attempt-specific
`--sensor-watch-startup-marker` (and may pass `--sensor-watch-version`,
`--sensor-watch-startup-attempt`, and `--sensor-watch-user-data`). Studio
consumes these bootstrap arguments before CLI dispatch, performs its failed
activation recovery check before distribution/resource/project initialization,
and writes the exact `ok\n` acknowledgement only after initialization completes.
The acknowledgement is written through a same-directory temporary file and
atomic rename, so a partial marker cannot be accepted. A supplied version
that does not match the running Studio is rejected and never acknowledged.

The launcher remains responsible for its bounded startup timeout and final
launcher-state pointer rollback. Studio never replaces the running executable;
new packages remain immutable version directories, while settings and projects
remain under user data.
