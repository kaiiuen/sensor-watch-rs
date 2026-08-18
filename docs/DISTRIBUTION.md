# Studio folder distribution foundation

This repository defines a bounded, folder-based Windows distribution contract
for Firmware Studio. It includes an offline/self-update foundation, but it does
not implement network downloads or in-place replacement of the running
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
through the typed `distribution` contract in `studio/src/distribution.rs`. The
supported entry point is the packaged launcher at
`launcher/sensor-watch-studio.exe`; users should not launch the versioned Studio
executable directly.

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

## Signed release metadata and A/B versions

Before selecting or starting a version, the launcher authenticates the signed
`release/metadata.json` using its configured key ring, checks the requested
version against that signed metadata, and verifies the selected executable's
artifact digest. SHA-256 is a digest/hash used to detect corruption or an
unexpected change; it is not a release key and does not establish who produced
an artifact. Authenticity requires a release signer to use a private signing key
and the launcher/distribution to verify the signature with the corresponding
pinned public verification key (for example, an Ed25519 key).

The trusted public key belongs in the launcher's configured key ring or another
protected, versioned distribution trust store, independently of mutable release
metadata or repository content. The private signing key must stay in the
protected release/signing environment and must never be committed, packaged, or
placed in the repository. A mutable GitHub branch, or a checksum fetched from
that branch alone, is not an authenticity root: an attacker who can change the
artifact can change its checksum too. Missing authentication, an invalid
signature, an untrusted version, a downgrade that is not allowed by policy, or
a digest mismatch fails closed. The package builder emits a
metadata/signature placeholder for local artifacts; it must be replaced with a
real signed release and a separately provisioned public key before publishing.

Installed Studio versions are immutable sibling directories under
`versions/<version>`. The launcher keeps A/B-style `current` and `previous`
pointers in `user-data/launcher-state.json`, updates that small state file
atomically, and
switches the pointers only after verification. A version that fails the startup
handshake is terminated and the previous verified version is restored. The
launcher itself is installed once and is not replaced while it is running.

## Building a package

`package-studio` automatically runs release builds for both the launcher package
`sensor-watch-launcher` and the Studio package `sensor-watch-studio`, then places
both artifacts in the package. A separately supplied launcher is supported for
tests or a release pipeline, but the normal command does not require a manual
launcher build.

## Limitations

This is an offline, folder-based distribution foundation. It does not download
updates, discover releases, or replace the running executable in place. The
package builder does not sign release metadata or bundle optional `tools` and
`targets` by default; publishing still requires an external signing step and
any required optional bundles must be supplied separately. The launcher owns
selection, verification, startup timeout, and rollback; Studio does not perform
those operations itself.
