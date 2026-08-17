# Repository Instructions

## Validation working directory and outputs

- Always run validation commands from the repository root (`sensor-watch-rs`), not from a parent directory or a temporary sibling directory.
- Use only a repository-local Cargo target directory: `target/validation/<operation>` for an isolated operation, or one explicitly named path under this repository's `target/`. Never set `CARGO_TARGET_DIR` to `Documents`, the parent repository, or any other parent path.
- Never create `rv-*`, `sensor-watch-validation-*`, or random sibling folders for builds, tests, logs, or artifacts.
- Do not use shell redirection that creates `nul`, `show-lines`, or other untracked output files. Use the editor/tool `head_lines` and `tail_lines` limits when output needs to be bounded.
- Clean generated validation caches under `target/validation/<operation>` after each run. Do not delete or overwrite canonical firmware, recovery, or user data, and do not delete ignored project logs.
- Report the exact artifact paths produced by every validation run, including ELF, UF2, manifest, signature, recovery, and log paths where applicable.

These rules prevent validation output from escaping the repository and keep canonical firmware, recovery, and user data plus ignored project logs intact. They do not authorize source edits, commits, pushes, resets, or repository-wide cleaning as part of validation.

## Canonical validation commands

Run each operation from the repository root. The examples use PowerShell and isolate Cargo output per operation. Use a different operation name only when it remains under `target/validation/`.

### Studio debug build and tests

```powershell
$env:CARGO_TARGET_DIR = 'target/validation/studio-debug'
cargo build -p sensor-watch-studio
cargo test -p sensor-watch-studio
cargo test -p sensor-watch-studio --no-default-features
```

Debug artifacts and test caches are under `target/validation/studio-debug/`.

### Release Studio build

```powershell
$env:CARGO_TARGET_DIR = 'target/validation/studio-release'
cargo build -p sensor-watch-studio --release
```

The release binary is:

```text
target/validation/studio-release/release/sensor-watch-studio.exe
```

(The executable suffix may differ on non-Windows hosts.)

### Workspace host tests

Use a host target explicitly so the embedded firmware is not accidentally tested with an ARM target:

```powershell
$env:CARGO_TARGET_DIR = 'target/validation/workspace-tests'
cargo test --workspace --target x86_64-pc-windows-msvc
```

For the CI-equivalent feature coverage, run the following in the same operation directory as needed:

```powershell
cargo test -p sensor-watch --lib --features hostmock,std
cargo test -p sensor-watch --lib --features hostmock,std,optical
cargo test -p sensor-watch --lib --features hostmock,std,shell-auth
cargo test -p sensor-watch --lib --features hostmock,std,optical,shell-auth,usb-cdc
cargo test -p sensor-watch-core --target x86_64-pc-windows-msvc
cargo test -p sensor-watch-tools --target x86_64-pc-windows-msvc
```

### ARM release wrapper

The wrapper supplies the repository linker configuration and builds the flashable ARM release image:

```powershell
$env:CARGO_TARGET_DIR = 'target/validation/arm-release'
& .\scripts\build-firmware.ps1
```

The preserved ARM ELF is:

```text
target/validation/arm-release/thumbv6m-none-eabi/release/sensor-watch
```

### UF2 build and verification

Build the stock UF2 and verify its structure and sidecars in one isolated operation:

```powershell
$env:CARGO_TARGET_DIR = 'target/validation/uf2-verification'
cargo run -p sensor-watch-tools -- build
cargo run -p sensor-watch-tools -- verify `
  target/validation/uf2-verification/thumbv6m-none-eabi/release/sensor-watch.uf2 `
  --manifest target/validation/uf2-verification/thumbv6m-none-eabi/release/sensor-watch.uf2.json
```

Report and preserve these exact artifacts when the operation is intentionally retained:

```text
target/validation/uf2-verification/thumbv6m-none-eabi/release/sensor-watch.uf2
target/validation/uf2-verification/thumbv6m-none-eabi/release/sensor-watch.uf2.json
target/validation/uf2-verification/thumbv6m-none-eabi/release/sensor-watch.uf2.json.sig
```

After recording the result and artifact paths, remove only disposable generated validation caches for that operation. Preserve any explicitly retained firmware or recovery outputs and ignored project logs.
