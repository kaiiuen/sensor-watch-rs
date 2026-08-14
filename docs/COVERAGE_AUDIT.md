# Test coverage and integration contract audit

**Audit date:** 2026-08-14  
**Scope:** workspace crates under `sensor-watch-rs`  
**Change policy:** this audit adds documentation only. No private files, reference
repositories, firmware implementation files, or generated artifacts were changed.

## Reproducible baseline

The following host commands were run from the repository root:

```text
cargo test --workspace --all-targets --no-fail-fast
cargo test -p sensor-watch --lib --features hostmock,std --no-fail-fast
cargo test -p sensor-watch --lib --features usb-cdc --no-fail-fast
cargo test -p sensor-watch --lib --features hostmock,std,optical,shell-auth,usb-cdc --no-fail-fast
```

The passing default workspace run reported:

| Package/path | Tests | Boundary covered |
|---|---:|---|
| `sensor-watch` host seam | 109 | Real firmware-face and host HAL seam behavior with `hostmock,std` |
| `sensor-watch-core` | 67 | Pure date/time, settings, UF2, ECC, transfer, optical, and safety logic |
| `sensor-watch-studio` | 91 | Studio default path, including the default `real-faces` bridge |
| `sensor-watch-tools` | 16 | Five library tests and eleven CLI tests |
| **Total** | **283** | Host/software coverage only |

The firmware host-seam run without `std` reports 108 tests; enabling `usb-cdc`
adds one descriptor-contract test, for 109 in the combined command. These are
not additional unique behaviors and should not be added to the 283 total.

No command in this audit validates real watch silicon, USB enumeration, UART
wiring, SWD probe behavior, power draw, or physical display/peripheral behavior.

## Concrete gaps found

### 1. Studio `real-faces` disabled mode is claimed but fails to compile

`studio/Cargo.toml` documents `real-faces` as optional, and
`studio/src/real_face.rs` documents a fallback implementation that returns
`None` so the simulator can use `face_sim`. The reproducible command:

```text
cargo test --manifest-path studio/Cargo.toml --no-default-features --no-fail-fast
```

fails before tests run because the fallback methods still return
`RealFaceSnapshot`, while that type is only defined under
`#[cfg(feature = "real-faces")]`:

```text
error[E0425]: cannot find type `RealFaceSnapshot` in this scope
studio/src/real_face.rs:899
studio/src/real_face.rs:929
studio/src/real_face.rs:900
```

**Impact:** the default Studio test suite passes while the advertised fallback
integration path is unbuildable. The fix belongs in the production seam (make
the snapshot API available to both configurations or provide an equivalent
fallback type); it was intentionally not made in this docs/tests-only audit.

### 2. Feature combinations are not CI-matrix coverage

CI exercises the default Studio feature set and the firmware host seam, but it
does not build/test the following combinations as explicit jobs:

- Studio `--no-default-features` (currently fails as described above).
- Firmware host seam with `hostmock` without `std` (the local run passes 108,
  but CI does not make this contract visible).
- Firmware host seam with `optical` combined with `hostmock,std` (the local
  combined command passes, but there is no dedicated regression job).
- Host compile/test behavior for the marker `shell-auth` feature. The feature
  is declared in `Cargo.toml`, but no source `cfg(feature = "shell-auth")`
  branch was found; current shell authorization tests exercise the host seam,
  not a feature-specific implementation.
- ARM compile coverage for `defmt-log`. The feature is explicitly ARM-only;
  host use is rejected by a compile error, but this audit environment did not
  run the ARM command because the required target/toolchain validation is a
  separate boundary from host tests.

The existing USB CDC contract is deliberately narrow: `--features usb-cdc`
checks descriptor constants only. It does not test controller transfers,
enumeration, suspend/resume, or power behavior, consistent with
`docs/USB_CDC.md`.

### 3. Claimed behaviors with no host substitute

The following status claims remain software-only or unexecuted, rather than
being covered by the passing test counts:

- real silicon boot, button/LCD/LED/buzzer/accelerometer/I2C behavior;
- current draw, RTC drift, brown-out, watchdog reset, and clock-failure recovery;
- USB CDC device enumeration and endpoint transfers;
- UART jig wiring and SWD/RTT probe behavior;
- Studio configured UF2 generation with user selections. The path is correctly
  fail-closed until selections become firmware build inputs;
- optical receiver integration. The protocol framing and authentication logic
  are host-testable, but no receiver integration is claimed.

## Stale documentation corrected by this audit

- The README baseline was `106 + 67 + 90 + 16 = 279`; the current verified
  baseline is `109 + 67 + 94 + 14 = 284`.
- The current Studio mapping uses 95 real firmware faces and 16 simulated
  faces out of 111.
- `PROJECT_LOG.md` contains private historical snapshots that are intentionally
not part of the public validation baseline.
  real faces). Those entries are retained as dated history, not treated as the
  current validation contract.

## Recommended follow-up gates

1. Fix the `real-faces`-off type-gating failure, then add a CI Studio
   `--no-default-features` job and a deterministic fallback test asserting
   `RealFace::new("SIMPLE_CLOCK") == None`.
2. Add a small CI feature matrix for `hostmock`, `std`, `optical`,
   `shell-auth`, and `usb-cdc`, distinguishing host contract tests from ARM
   compile checks.
3. Keep physical validation results in `docs/TESTING.md` separate from host
   test counts and record each as PASS, FAIL, NOT AVAILABLE, or NOT TESTED.
